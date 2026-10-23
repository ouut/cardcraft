# CardCraft AI Agent API Reference & Scheme Game Engine Guide

> **面向 AI Agent 的专门技术规范文档**  
> 本文档旨在为大型语言模型（LLM / AI Agent / Coding Assistant）提供结构化、精确、无歧义的 CardCraft 运行机制说明。阅读本文档后，AI Agent 可直接独立编写、扩展和调试基于 CardCraft 的 Scheme 游戏业务逻辑。

---

## 1. 核心系统心智模型 (System Mental Model)

CardCraft 采用**“窄腰沙漏（Narrow-Waist）”双核解耦架构**：

```
[外部输入 / 客户端]
  ├─ 终端键盘输入 (Crossterm)
  ├─ 浏览器 WebSocket (JSON 消息)
  ├─ 裸 TCP 套接字 (行协议)
  └─ Iroh P2P QUIC 隧道
           │
           ▼ (CAD 字符串命令)
┌─────────────────────────────────────────────────────────────┐
│                    Rust 宿主引擎 (Host Engine)               │
│  - 负责多协议网络 I/O、静态文件托管、Actor 线程模型与沙盒防护   │
│  - 不维护任何游戏玩法状态（Zero Game State）                  │
└─────────────────────────────────────────────────────────────┘
           │                                 ▲
           │ 1. 宿主原语注入 (vm.register_fn)  │ 3. 抽取只读快照 (JSON)
           ▼                                 │
┌─────────────────────────────────────────────────────────────┐
│                Steel Scheme 虚拟机 (Guest Logic)            │
│  - 游戏世界的唯一事实源 (Single Source of Truth)              │
│  - 100% 掌管实体、卡牌、配方、合成倒计时、月相消耗与网络业务    │
└─────────────────────────────────────────────────────────────┘
```

### 关键架构原则：
1. **Scheme 是唯一状态源**：卡牌位置、金币数量、配方匹配、倒计时、月相消耗完全在 Scheme 空间内执行与存储。
2. **命令驱动与时间步长**：
   - 用户输入由 Scheme 函数 `(game-input . rest)` 处理并返回结果字符串（Feedback）。
   - 时间流逝由定时器周期调用 `(game-update! dt)` 推进（`dt` 单位为秒，默认 0.1s）。
3. **快照视图单向导出**：
   - Rust 每帧调用 `(game-render)` 获取 Scheme 序列化后的只读 JSON 字符串，反序列化为 `GameViewSnapshot`，用于终端渲染或推送到前端。

---

## 2. 游戏目录结构与沙盒保护

通过命令行参数 `-g, --game <DIR>` 指定游戏工作目录（默认为当前目录 `.`）。每个游戏为一个完整的自包含资产包：

```text
<game_dir>/
├── core.scm              # [优先级 1] 游戏主业务入口
├── main.scm              # [优先级 2] 备选入口（若无 core.scm）
├── saves/                # [沙盒隔离目录] 由引擎自动创建（存储存档数据）
│   └── slot1.json
└── web/                  # [Web 客户端] 该游戏的专属网页前端（引擎自动探测与 HTTP 托管）
    ├── index.html
    └── style.css
```

> [!IMPORTANT]
> **沙盒文件隔离规则 (Path Traversal Jail)**：
> 游戏脚本调用 `(system-file-write path content)` 或 `(system-file-read path)` 时，路径被严格限定在 `<game_dir>/saves/` 内。任何使用 `../`、`../../` 试图逃逸至上层目录的读写均会被 Rust 宿主拦截并拒绝。
>
> **Web 客户端自动探测**：
> 若 `<game_dir>/web/` 存在，Web 服务自动托管该目录；若不存在，自动回退使用全局 `./web/` 控制台前端。可通过 `--serve <DIR>` 手动指定任意外部路径。

---

## 3. Rust 注入的宿主系统原语手册 (System Host Primitives)

在执行 Scheme 游戏脚本前，Rust 宿主在全局环境中注册了标准化的 `system-*` 核心原语体系（提供向后兼容别名）：

| 原语名称 | 参数类型 | 返回值 | 语义说明与典型用法 |
| :--- | :--- | :--- | :--- |
| `(system-log level msg)` | `String, String` | `#<void>` | 输出日志至终端标准错误。`level` 可为 `"INFO"`、`"WARN"`、`"ERROR"`。（兼容别名：`rust-log`） |
| `(system-utils-random-int min max)` | `Integer, Integer` | `Integer` | 生成闭区间 `[min, max]` 的随机整数（基于硬件随机数发生器）。（兼容别名：`rust-random-int`） |
| `(system-window-size)` | 无 | `'(width height)` | 获取当前物理终端视口的宽高（字符列数与行数），返回二元列表。用 `(car (system-window-size))` 取得宽，`(cadr (system-window-size))` 取得高。（兼容别名：`system-terminal-width`, `system-terminal-height`） |
| `(system-time-now)` | 无 | `Number` | 获取当前自 Unix 纪元起的高精度浮点秒数 (`f64`)，已合并原 `rust-millis`，用于高精度耗时与冷却判定。 |
| `(system-file-write path content)` | `String, String` | `Boolean` | 将字符串写入 `<game_dir>/saves/<path>`。成功返回 `#true`，越界防护返回 `#false`。 |
| `(system-file-read path)` | `String` | `String` | 从 `<game_dir>/saves/<path>` 读取文本内容。文件不存在或失败时返回 `""`。 |
| `(system-net-send! cid message)` | `Integer, String` | `Boolean` | 向指定 `client-id` 的已连接客户端单播文本消息。（兼容别名：`net-send!`） |
| `(system-net-broadcast! message)` | `String` | `Boolean` | 向所有当前在线的网络客户端广播文本消息。（兼容别名：`net-broadcast!`） |
| `(system-net-whisper! from to message)` | `Integer, Integer, String` | `Boolean` | 从 `from-cid` 向 `to-cid` 派发私聊密语事件。（兼容别名：`net-whisper!`） |
| `(system-net-disconnect! cid)` | `Integer` | `Boolean` | 强制切断指定 `client-id` 的客户端网络连接。（兼容别名：`net-disconnect!`） |

---

## 4. Scheme 必须实现的生命周期钩子 (Required Hooks)

编写一个 CardCraft 游戏，Scheme 脚本必须（或建议）定义以下标准顶层函数：

### 4.1 核心单机与引擎驱动钩子

#### 1. `(game-start!)` 或 `(game-init!)`
- **调用时机**：脚本加载完成后，引擎初始化时触发一次。
- **职责**：重置卡池、清空场上卡栈、设定初始金币、村民、月相时间。

#### 2. `(game-update! dt)`
- **调用时机**：由游戏循环每 100ms 触发一次（`dt` 通常为 `0.1` 秒）。
- **职责**：
  1. 推进正在合成（Crafting）的卡栈进度计时器：`progress = progress + dt`。
  2. 计时器完成（`progress >= total_time`）时，消耗原材料卡，生成产物卡。
  3. 推进月相/昼夜计时：扣除食物，若食物不足触发饥饿/惩罚事件。
- **返回值**：忽略。

#### 3. `(game-input . rest)`
- **调用时机**：用户在控制台按下回车，或网络客户端提交指令时触发。
- **参数形式**：
  - 形式 A（单机控制台）：`(game-input tokens)`，如 `tokens = '("s" "1" "2")`。
  - 形式 B（网络客户端）：`(game-input client-id tokens)`。
- **职责**：解析动词与参数，执行对应业务（移动、堆叠、拆包、买卖），更新世界状态。
- **返回值**：`String`，作为操作执行后的反馈文本（Feedback），例如 `"OK"`、`"Produced: Berry"` 或 `"Error: Slot not found"`。

#### 4. `(game-render)`
- **调用时机**：每次画面需要重绘或生成快照推送给前端时调用。
- **职责**：将当前世界状态导出为符合 `GameViewSnapshot` 规范的 **JSON 格式字符串**。
- **返回值**：`String`（合法的 JSON 字符串）。

---

### 4.2 网络多人事件钩子 (Multiplayer Hooks)

当通过 `--ws`、`--tcp`、或 P2P 模式启动服务器时，网络事件会回调以下 Scheme 钩子：

```scheme
;; 1. 当新客户端连入时触发
(define (net-on-connect! cid)
  (let ([name (string-append "Player_" (number->string cid))])
    (system-net-send! cid (string-append "Welcome! You are " name))
    (system-net-broadcast! (string-append name " joined."))
    "OK"))

;; 2. 当客户端提交任意非空文本时触发
(define (net-on-message! cid msg-str)
  (let ([tokens (split-spaces msg-str)])
    (cond
      [(equal? (car tokens) "say") 
       (system-net-broadcast! (string-append "Player_" (number->string cid) ": " msg-str))
       "OK"]
      [else 
       ;; 转发给游戏 CAD 输入处理
       (game-input cid tokens)])))

;; 3. 当客户端断开连接时触发
(define (net-on-disconnect! cid)
  (system-net-broadcast! (string-append "Player_" (number->string cid) " disconnected."))
  "OK")
```

---

## 5. 快照视图 JSON 数据契约 (GameViewSnapshot Contract)

CardCraft 遵循**“全屏纯字符网格接管”**哲学：Scheme 拥有终端与画布的 **100% 渲染主权**。Rust 宿主端无任何固化的布局窗格或业务模板，仅将 `(game-render)` 返回的字符串网格逐行投影到物理终端或虚拟显存。

### 5.1 全屏字符网格规范 (Full-Screen Canvas Schema)

`(game-render)` 导出的 JSON 结构极为简洁纯粹：

```json
{
  "cursor": [11, 8],
  "grid": [
    "+-- CardCraft Console ----------------------------------------+",
    "| MOON 1 | [==........] 25s | Food: 2/1 | Gold: 5G | [RUNNING] |",
    "+-- Board & Stacks ------------------------------------------+",
    "| [ 2] BerryBush Villager [........] (4s / Harvest Berry)      |",
    "| [ 3] Berry                                                  |",
    "+-- Actions & Store -----------------------------------------+",
    "| ACTIONS: [b basic] Basic Pack (3G)  |  [b food] Food Pack (5G) |",
    "+-- Command Prompt [OK] --------------------------------------+",
    "| Command: s 1 2                                              |",
    "+-------------------------------------------------------------+"
  ]
}
```

### 字段说明：
- **`grid`** (`Vec<String>`): 字符行列表。从第 0 行自顶向下铺满视口。支持通过 `(system-window-size)` 动态获取终端宽度与高度进行响应式居中或排版。
- **`cursor`** (`Option<[u16; 2]>`): 硬件物理光标位置 `[x, y]`（列与行坐标，0-indexed）。如果为 `null`，宿主自动隐藏物理光标。

### 5.2 轻量行内颜色高亮标签 (Inline Color Tags)
在 `grid` 每一行中，Scheme 可以自由嵌入用方括号包围的色彩修饰标签，Rust 渲染器会自动解析并映射至 ANSI 色彩空间：
- **前景色**：`[black]`, `[red]`, `[green]`, `[yellow]`, `[blue]`, `[magenta]`, `[cyan]`, `[white]`
- **修饰符**：`[bold]`
- **重置**：`[reset]`

示例：
```scheme
"|.........[yellow]@[reset]...................[red]GoblinKing[reset]..........|"
```

---

## 6. 统一硬件事件与网络分发流水线 (Unified Event Stream)

所有键盘、鼠标、窗口大小变动与网络消息统一通过唯一的通用入口派发给 Scheme：

```scheme
(net-on-message! cid message-str)
```

### 6.1 本地终端事件 (cid = 0)
- **按键事件**：前缀为 `"key:"`，包括：
  - 功能键：`"key:enter"`, `"key:backspace"`, `"key:space"`, `"key:left"`, `"key:right"`, `"key:up"`, `"key:down"`, `"key:home"`, `"key:end"`
  - 普通按键：`"key:a"`, `"key:1"`, `"key:w"`, `"key:s"` 等
- **窗口大小变动事件**：前缀为 `"resize:"`，例如 `"resize:120x35"`（表示当前终端变更为 120 列 x 35 行）。
- **鼠标事件**：前缀为 `"mouse:"`，例如 `"mouse:down:10,5"`, `"mouse:up:10,5"`, `"mouse:drag:12,5"`。

### 6.2 远程网络客户端消息 (cid > 0)
- 网络客户端（WebSocket、TCP、P2P）发送的文本命令，如 `"s 1 2"`, `"say Hello World"`, `"who"` 等。

### 6.3 典型分发实现示例
```scheme
(define *tui-input-buffer* "")
(define *tui-last-feedback* "Ready")

(define (net-on-message! cid msg)
  (cond
    ;; 1. 本地键盘事件直通
    [(starts-with? msg "key:")
     (let ([k (substring msg 4 (string-length msg))])
       (cond
         [(equal? k "enter")
          (let ([cmd *tui-input-buffer*])
            (set! *tui-input-buffer* "")
            (dispatch-command! cid cmd))]
         [(equal? k "backspace")
          (let ([len (string-length *tui-input-buffer*)])
            (when (> len 0)
              (set! *tui-input-buffer* (substring *tui-input-buffer* 0 (- len 1))))
            "OK")]
         [(equal? k "space")
          (set! *tui-input-buffer* (string-append *tui-input-buffer* " "))
          "OK"]
         [(= (string-length k) 1)
          (set! *tui-input-buffer* (string-append *tui-input-buffer* k))
          "OK"]
         [else "OK"]))]

    ;; 2. 窗口重排事件
    [(starts-with? msg "resize:") "OK"]

    ;; 3. 鼠标交互事件
    [(starts-with? msg "mouse:") "OK"]

    ;; 4. 网络文本命令
    [else (dispatch-command! cid msg)]))
```

---

## 7. 标准 CAD 命令行分发规范

建议游戏脚本支持以下简洁的动词缩写体系：

| 指令缩写 | 完整动词 | 语法形式 | 说明 |
| :--- | :--- | :--- | :--- |
| `s` | `stack` | `s <from-id> <to-id>` | 将卡槽 `from-id` 压入 `to-id`，触发配方模式匹配 |
| `u` | `unstack`| `u <id>` | 将卡槽 `id` 的顶端卡弹出并放入新的可用卡槽 |
| `m` | `move` | `m <from-id> <to-id>` | 将卡槽整体移动到指定的空闲卡槽 |
| `b` | `buy` | `b <pack-id>` | 扣除金币购买卡包 |
| `o` | `open` | `o <id>` | 拆解卡槽上的卡包，随机迸发资源卡 |
| `x` | `sell` | `x <id>` | 出售卡槽卡牌，回收金币 |
| `w` | `wait` | `w [seconds]` | 快进时间流动（如 `w 5` 快进 5 秒） |
| `p` | `pause` | `p` | 暂停 / 继续时间步长推进 |
| `(...)`| Scheme | `(eval-expr ...)` | 若指令以括号开头，系统将直接作为原生 Scheme 代码动态求值 |

---

## 7. 极简可运行游戏模板 (Minimal Complete Game Template)

这是一个完整的、可直接复制到新目录运行的极简 Scheme 游戏实现：

```scheme
;; ===================================================================
;; minimal_game/core.scm - 极简独立游戏样例
;; ===================================================================

(define *gold* 10)
(define *moon* 1)
(define *timer* 0.0)
(define *logs* '("Game started!"))
(define *stacks* 
  '((1 ("Hero") #f #f #f "Idle")
    (2 ("Slime") #f #f #f "Idle")
    (3 ("Coin") #f #f #f "Idle")))

;; 辅助函数：添加日志
(define (add-log! msg)
  (set! *logs* (cons msg *logs*)))

;; 1. 游戏初始化
(define (game-start!)
  (set! *gold* 10)
  (set! *moon* 1)
  (set! *timer* 0.0)
  (add-log! "Ready for battle!"))

;; 2. 时间演进
(define (game-update! dt)
  (set! *timer* (+ *timer* dt))
  (if (>= *timer* 30.0)
      (begin
        (set! *moon* (+ *moon* 1))
        (set! *timer* 0.0)
        (add-log! (string-append "Moon " (number->string *moon*) " has arrived!")))
      #t))

;; 3. CAD 指令输入
(define (game-input . rest)
  (let* ([tokens (if (> (length rest) 1) (cadr rest) (car rest))])
    (if (null? tokens)
        "Empty command"
        (let ([verb (car tokens)])
          (cond
            [(equal? verb "help") "Commands: s <from> <to>, b hero, save, load"]
            [(equal? verb "save")
             (system-file-write "save.json" (game-render))
             "Game saved to saves/save.json!"]
            [(equal? verb "load")
             (let ([data (system-file-read "save.json")])
               (if (equal? data "")
                   "No save file found."
                   (begin
                     (add-log! "Save loaded.")
                     "Save loaded!")))]
            [else "Command processed: OK"])))))

;; 4. 视图导出 (生成符合规范的 JSON)
(define (game-render)
  (string-append
    "{"
      "\"meta\": {"
        "\"moon\": " (number->string *moon*) ","
        "\"day_progress\": " (number->string (/ *timer* 30.0)) ","
        "\"day_seconds_left\": " (number->string (- 30.0 *timer*)) ","
        "\"gold\": " (number->string *gold*) ","
        "\"food_current\": 5,"
        "\"food_required\": 1,"
        "\"card_count\": " (number->string (length *stacks*)) ","
        "\"card_limit\": 20,"
        "\"status\": \"Running\""
      "},"
      "\"stacks\": ["
        "{\"id\": 1, \"cards\": [\"Hero\"], \"progress\": null, \"total_time\": null, \"recipe_name\": null, \"status\": \"Idle\"},"
        "{\"id\": 2, \"cards\": [\"Slime\"], \"progress\": null, \"total_time\": null, \"recipe_name\": null, \"status\": \"Idle\"}"
      "],"
      "\"shop\": ["
        "{\"id\": \"potion\", \"name\": \"Health Potion\", \"cost\": 5}"
      "],"
      "\"logs\": ["
        "\"" (if (null? *logs*) "" (car *logs*)) "\""
      "]"
    "}"))

;; 5. 网络连接钩子
(define (net-on-connect! cid)
  (system-net-send! cid "Welcome to Minimal RPG!")
  "OK")

(define (net-on-message! cid msg)
  (game-input cid (list msg)))

(define (net-on-disconnect! cid)
  "OK")
```

---

## 8. AI Agent 开发与自测验证指南

AI Agent 在生成或修改游戏业务逻辑后，可通过以下工作流进行无头（Headless）自动化自测与验证：

### Step 1: 语法与渲染校验 (Headless Run)
```bash
cargo run -- -g ./games/my_game --headless
```
- **通过标志**：控制台正确输出首帧 80x25 虚拟终端字符渲染框，未抛出 Scheme 执行异常。

### Step 2: 启动本地 WebSocket 验证全链路
```bash
cargo run -- -g ./games/my_game --ws 8080 --headless
```
- 使用 curl 发起 HTTP GET 请求验证静态服务：
  ```bash
  curl -I http://localhost:8080/
  # 应返回: HTTP/1.1 200 OK
  ```

### Step 3: 测试沙盒安全性
编写测试验证 `(system-file-write "../../hack.txt" "content")` 是否被 Rust 沙盒防护拦截。

### Step 4: 运行全套单元测试
```bash
cargo test
```
确保新增的 Scheme 代码未破坏宿主引擎的 20 项单元与集成测试。
