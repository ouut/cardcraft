# CardCraft 开发者指南：Rust 与 Steel Scheme 互操作与扩展

本文档面向想要深入二次开发、扩充游戏引擎能力或编写高级 Mod 的开发者。详细说明 **`core.scm` 能访问哪些 Rust 接口**、**如何在 Rust 中为 Scheme 注册新的宿主接口**，以及**完整的双向调用与开发测试流程**。

---

## 一、 架构总览：双向互操作模型

CardCraft 采用双向互操作设计：

```text
┌─────────────────────────────────────────────────────────────┐
│                    Rust 宿主环境 (Host)                      │
│                                                             │
│   [Engine::new()]                                           │
│         │                                                   │
│         │ 1. 注册宿主能力 (vm.register_fn)                   │
│         ▼                                                   │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 暴露的 Rust 原语：                                   │   │
│   │ • (rust-log level msg)                              │   │
│   │ • (rust-random-int min max)                         │   │
│   │ • (rust-millis)                                     │   │
│   │ • (system-file-write path content)                  │   │
│   │ • (system-file-read path)                           │   │
│   └─────────────────────────────────────────────────────┘   │
│         │                                                   │
│         │ 2. 注入并执行 core.scm                             │
│         ▼                                                   │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ Steel Scheme 解释器空间 (Guest VM)                  │   │
│   │                                                     │   │
│   │   * (game-start!)                                   │   │
│   │   * (game-update! dt)                               │   │
│   │   * (game-input '("s" "1" "2"))                     │   │
│   │   * (game-render)                                   │   │
│   └─────────────────────────────────────────────────────┘   │
│         ▲                                                   │
│         │ 3. 抽取只读 JSON 快照 (Serde 反序列化)              │
│         │                                                   │
│   [GameViewSnapshot 虚拟缓冲渲染 / TUI 显示]                 │
└─────────────────────────────────────────────────────────────┘
```

---

## 二、 `core.scm` 当前可访问的 Rust 接口

在 `src/engine.rs` 中，Rust 引擎向 Scheme 空间直接注入了以下标准宿主函数：

### 1. 高熵随机数生成器：`(rust-random-int min max)`
* **说明**：基于 Rust `rand` 库的高性能随机数，闭区间 `[min, max]`。
* **参数**：
  * `min` (Integer): 最小值（例如 `0`）。
  * `max` (Integer): 最大值（例如 `4`）。
* **返回值**：`(Integer)` 生成的随机整数。
* **在 `core.scm` 中的实际应用**：
  ```scheme
  ;; 从列表中随机抽取一项
  (define (random-choice lst)
    (if (null? lst)
        #f
        (let* ([len (length lst)]
               [idx (rust-random-int 0 (- len 1))])
          (list-ref lst idx))))

  ;; 开启基础卡包时随机掉落
  (let* ([pool '("Wood" "Stone" "BerryBush" "Berry" "Coin")]
         [drop (random-choice pool)])
    (spawn-card! drop))
  ```

---

### 2. 高精度时间戳：`(rust-millis)`
* **说明**：获取当前系统自 Unix 纪元起的毫秒级时间戳。
* **参数**：无。
* **返回值**：`(Integer)` 当前毫秒数（`i64`）。
* **示例**：
  ```scheme
  (let ([start (rust-millis)])
    ;; 执行某项大列表计算...
    (let ([cost (- (rust-millis) start)])
      (rust-log "INFO" (string-append "Calculation took " (number->string cost) "ms"))))
  ```

---

### 3. 系统日志输出：`(rust-log level msg)`
* **说明**：向宿主标准错误（stderr）或后台日志管道打印格式化日志。
* **参数**：
  * `level` (String): 日志级别，如 `"DEBUG"`、`"INFO"`、`"WARN"`、`"ERROR"`。
  * `msg` (String): 日志正文。
* **返回值**：`#<void>`。
* **示例**：
  ```scheme
  (rust-log "WARN" "Villager is starving!")
  ```

---

### 4. 文件写入：`(system-file-write path content)`
* **说明**：将字符串保存至本地磁盘文件（用于存档或导出游戏数据）。
* **参数**：
  * `path` (String): 文件绝对或相对路径。
  * `content` (String): 写入的内容（通常是 JSON 或 S 表达式）。
* **返回值**：`(Boolean)` 写入成功返回 `#true`，失败返回 `#false`。
* **示例**：
  ```scheme
  (system-file-write "./saves/slot1.json" (game-render))
  ```

---

### 5. 文件读取：`(system-file-read path)`
* **说明**：从本地磁盘读取文本文件（用于读档或加载外部扩展规则）。
* **参数**：
  * `path` (String): 文件路径。
* **返回值**：`(String)` 文件内容；若读取失败返回空字符串 `""`。
* **示例**：
  ```scheme
  (let ([mod-code (system-file-read "./mods/extra_recipes.scm")])
    (display mod-code))
  ```

---

### 6. 网络通信 4 大原语与 3 大事件钩子 (The 7 Network Primitives & Hooks)

CardCraft 将所有网络与多人业务（聊天、私聊、房间、匹配、游戏指令路由）下放至 Scheme，Rust 宿主仅负责高并发套接字搬运：

#### (1) Rust 注入的 4 大网络原语
* **`(net-send! client-id message)`**：向指定的已连接客户端单播消息。
* **`(net-broadcast! message)`**：向当前在线的所有客户端广播消息。
* **`(net-disconnect! client-id)`**：强制断开指定客户端连接。
* **`(system-time-now)`**：获取当前系统自 Unix 纪元起的高精度浮点秒数（`f64`），便于 Scheme 内置超时或计时器判定。

#### (2) Scheme 响应的 3 大事件钩子
* **`(net-on-connect! client-id)`**：当新客户端连入时自动触发，完成默认角色分配与在线表注册。
* **`(net-on-disconnect! client-id)`**：当客户端断开连接时自动触发，清理在线表并广播离线消息。
* **`(net-on-message! client-id message-string)`**：当收到客户端的文本包时触发，内置支持 `say`（公屏聊天）、`w`（私聊密语）、`nick`（改名）、`who`（查在线列表）以及 CAD 游戏指令转发。

> [!TIP]
> 详细网络业务实现场景（聊天室、私聊、房间隔离、天梯匹配、交易行、防刷频与风控）与完整 Scheme 源码，请参阅专属技术文档：[NETWORK_MODULE.md](NETWORK_MODULE.md)。

---

## 三、 开发者实战：如何给 Scheme 注册新的 Rust 接口？

如果你想让 Scheme 支持更多底层能力（例如：音频音效、网络请求、SQLite 存储、物理震动），只需在 Rust 中使用 `RegisterFn` 注册函数即可。

### 1. 接口类型映射对照表

Steel Scheme 支持自动进行原生类型双向转换：

| Rust 类型 | Steel Scheme 类型 | 说明 |
| :--- | :--- | :--- |
| `i64` / `isize` / `i32` | `Integer` | 整数 |
| `f64` / `f32` | `Number` | 浮点数 |
| `bool` | `Boolean` (`#true` / `#false`) | 布尔值 |
| `String` / `&str` | `String` | 字符串 |
| `Vec<T>` | `List` | 列表（若 `T` 可转换） |
| `()` | `#<void>` | 空返回值 |

---

### 2. 步骤 1：在 `src/engine.rs` 引入 `RegisterFn` 并注册

打开 `src/engine.rs`，在 `GameEngine::new()` 中使用 `vm.register_fn` 注入函数：

```rust
use steel::steel_vm::register_fn::RegisterFn;

impl GameEngine {
    pub fn new() -> Result<Self> {
        let mut vm = Engine::new();

        // 示例 1: 注册一个音效播放接口
        vm.register_fn("rust-play-sound", |sound_name: String| {
            // 调用 rodio 或系统音频后端播放音效
            eprintln!("[AUDIO]: Playing sound asset -> {sound_name}.wav");
        });

        // 示例 2: 注册一个求质数计算的重计算函数
        vm.register_fn("rust-is-prime", |n: i64| -> bool {
            if n <= 1 { return false; }
            for i in 2..=((n as f64).sqrt() as i64) {
                if n % i == 0 { return false; }
            }
            true
        });

        // ... 后续加载 core.scm ...
    }
}
```

---

### 3. 步骤 2：在 Scheme 中无缝调用

注册后，无需任何额外配置，`core.scm` 或用户在终端 CAD 命令行中即可直接像调用普通 Scheme 函数一样使用：

```scheme
;; 在合成成功时触发音效
(if (equal? recipe-name "Harvest Berry")
    (rust-play-sound "harvest_pop"))

;; 调用 Rust 高性能计算
(if (rust-is-prime 104729)
    (add-log! "Discovered a prime easter egg!"))
```

---

## 四、 Rust 如何调用 Scheme 层的接口？

从 Rust 调用 Scheme 只需要使用 `self.vm.run(code)`：

### 1. 无参驱动调用
```rust
// 推进世界时钟 0.1 秒
self.vm.run("(game-update! 0.1)")?;
```

### 2. 传参驱动调用
```rust
// 执行用户输入的 CAD 命令（参数自动转为 Scheme 列表）
let code = format!("(game-input '({}))", tokens_str);
let res = self.vm.run(code)?;
```

### 3. 提取返回值并反序列化（极窄接口模型）
Scheme 侧统一将展示数据打包为 JSON 字符串返回：
```rust
let res = self.vm.run("(game-render)")?;
if let Some(steel::rvals::SteelVal::StringV(json_str)) = res.last() {
    let snapshot: GameViewSnapshot = serde_json::from_str(json_str)?;
    // 将快照送往虚拟缓冲层渲染...
}
```

---

## 五、 开发与调试技巧

### 1. 利用终端自带的 Lisp God Mode 实时联调
在运行游戏时（`cargo run`），底部的命令行除了支持 `s 1 2` 这类 CAD 简写指令外，**支持输入任意 Scheme 表达式**：

* 输入 `(rust-random-int 1 100)`：立即看到随机数返回。
* 输入 `(rust-log "TEST" "debugging something")`：观察 stderr 输出。
* 输入 `(spawn-card! "Apple")`：实时向当前场上注入卡牌。

### 2. 编写无头单测（Headless Tests）
在 `tests/headless_tests.rs` 中增加单测验证你的新接口，毫秒级得出结果：

```rust
#[test]
fn test_my_custom_binding() {
    let mut engine = GameEngine::new().unwrap();
    let res = engine.execute_command("(rust-random-int 1 10)").unwrap();
    let n: i64 = res.parse().unwrap();
    assert!(n >= 1 && n <= 10);
}
```

运行测试：
```bash
cargo test --test headless_tests
```
