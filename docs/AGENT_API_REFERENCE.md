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

通过命令行参数 `-g, --game <DIR>` 指定游戏工作目录（默认为当前目录 `.`）：

```text
<game_dir>/
├── core.scm              # [优先级 1] 游戏主业务入口
├── main.scm              # [优先级 2] 备选入口（若无 core.scm）
└── saves/                # [沙盒隔离目录] 由引擎自动创建
    └── slot1.json        # 存档与持久化数据（严格限制在此目录中）
```

> [!IMPORTANT]
> **沙盒文件隔离规则 (Path Traversal Jail)**：
> 游戏脚本调用 `(system-file-write path content)` 或 `(system-file-read path)` 时，路径被严格限定在 `<game_dir>/saves/` 内。任何使用 `../`、`../../` 试图逃逸至上层目录的读写均会被 Rust 宿主拦截并拒绝。

---

## 3. Rust 注入的宿主原语手册 (Host Primitives)

在执行 Scheme 游戏脚本前，Rust 宿主会在全局环境中注册以下 10 个核心原语：

| 原语名称 | 参数类型 | 返回值 | 语义说明与典型用法 |
| :--- | :--- | :--- | :--- |
| `(rust-log level msg)` | `String, String` | `#<void>` | 输出日志至终端标准错误。`level` 可为 `"INFO"`、`"WARN"`、`"ERROR"`。 |
| `(rust-random-int min max)` | `Integer, Integer` | `Integer` | 生成闭区间 `[min, max]` 的随机整数（基于 Rust 硬件随机数生成器）。 |
| `(rust-millis)` | 无 | `Integer` | 获取当前自 Unix 纪元起的毫秒时间戳 (`i64`)。 |
| `(system-time-now)` | 无 | `Number` | 获取当前自 Unix 纪元起的高精度浮点秒数 (`f64`)。 |
| `(system-file-write path content)` | `String, String` | `Boolean` | 将字符串写入 `<game_dir>/saves/<path>`。成功返回 `#true`，失败返回 `#false`。 |
| `(system-file-read path)` | `String` | `String` | 从 `<game_dir>/saves/<path>` 读取文本内容。文件不存在或失败时返回 `""`。 |
| `(net-send! cid message)` | `Integer, String` | `#<void>` | 向指定 `client-id` 的已连接网络客户端单播文本消息。 |
| `(net-broadcast! message)` | `String` | `#<void>` | 向所有当前在线的网络客户端广播文本消息。 |
| `(net-whisper! from to message)` | `Integer, Integer, String` | `#<void>` | 从 `from-cid` 向 `to-cid` 发送私聊消息。 |
| `(net-disconnect! cid)` | `Integer` | `#<void>` | 强制断开指定 `client-id` 的客户端网络连接。 |

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
    (net-send! cid (string-append "Welcome! You are " name))
    (net-broadcast! (string-append name " joined."))
    "OK"))

;; 2. 当客户端提交任意非空文本时触发
(define (net-on-message! cid msg-str)
  (let ([tokens (split-spaces msg-str)])
    (cond
      [(equal? (car tokens) "say") 
       (net-broadcast! (string-append "Player_" (number->string cid) ": " msg-str))
       "OK"]
      [else 
       ;; 转发给游戏 CAD 输入处理
       (game-input cid tokens)])))

;; 3. 当客户端断开连接时触发
(define (net-on-disconnect! cid)
  (net-broadcast! (string-append "Player_" (number->string cid) " disconnected."))
  "OK")
```

---

## 5. 快照视图 JSON 数据契约 (GameViewSnapshot Contract)

`(game-render)` 导出的 JSON 会被 Rust 宿主反序列化为 `GameViewSnapshot`。引擎已实现**100% 动态泛化**，绝不强制绑定任何特定玩法概念（如月相、食物、金币或商店）。

### 5.1 通用游戏快照推荐格式 (Generic Schema)
适用于任何卡牌、RPG、自走棋、策略或模拟经营游戏：

```json
{
  "title": "Dungeon Crawler RPG",
  "metrics": [
    { "label": "HP", "value": "85/100", "color": "green" },
    { "label": "FLOOR", "value": "B3", "color": "cyan" },
    { "label": "GOLD", "value": "50G", "color": "yellow" }
  ],
  "stacks": [
    {
      "id": 1,
      "cards": ["Warrior", "Excalibur"],
      "progress": 0.5,
      "total_time": 4.0,
      "recipe_name": "Slash Attack",
      "status": "Combat"
    },
    {
      "id": 2,
      "cards": ["GoblinKing"],
      "progress": null,
      "total_time": null,
      "recipe_name": null,
      "status": "Hostile"
    }
  ],
  "panels": [
    { "id": "atk", "name": "Attack" },
    { "id": "cast", "name": "Cast Fireball", "cost": 10 }
  ],
  "panel_title": "Quick Actions",
  "logs": [
    "Encountered Goblin King on Floor B3!"
  ]
}
```

### 字段说明：
- **`title`** (`Option<String>`): 顶栏窗口自定义游戏标题。
- **`metrics`** (`Vec<MetricItem>`): 顶栏徽章列表，每项包含 `label`、`value` 和可选 `color`（`"red"`, `"green"`, `"yellow"`, `"cyan"`, `"magenta"`, `"blue"`）。由 Scheme 动态定义，无任何硬编码指标。
- **`stacks`** (`Vec<StackView>`): 场上卡槽/实体列表，包含 `id`, `cards`, `progress`, `total_time`, `recipe_name`, `status`。
- **`panels`** (`Vec<PanelItem>`，别名 `shop`): 可选操作栏/商店条目列表。若为空，宿主 TUI 自动自适应折叠该面板，让位给卡牌区。
- **`logs`** (`Vec<String>`): 事件与聊天日志。

### 5.2 兼容支持格式 (Legacy Schema)
若未提供 `metrics`，引擎自动兼容读取 `meta`（`moon`, `gold`, `food_current`, etc.）与 `shop`，确保原型脚本与单元测试无缝运行。
- `meta.food_required`: `i32`，月末村民消耗的总食物点数。
- `meta.card_count`: `i32`，场上卡牌总数。
- `meta.card_limit`: `i32`，卡牌上限。
- `meta.status`: `String`，游戏当前状态（`"Running"`、`"Paused"`、`"GameOver"` 等）。
- `stacks`: 数组，每个元素包含：
  - `id`: `i32`，卡栈槽位编号。
  - `cards`: `Vec<String>`，卡栈内包含的卡牌名称列表（底端卡在前面，顶端卡在后面）。
  - `progress`: `Option<f64>`，当前合成进度秒数（未合成时为 `null`）。
  - `total_time`: `Option<f64>`，合成所需总秒数（未合成时为 `null`）。
  - `recipe_name`: `Option<String>`，命中配方的名称（未合成时为 `null`）。
  - `status`: `String`，卡栈状态，通常为 `"Idle"` 或 `"Crafting"`。
- `shop`: 数组，商店卡包列表，包含 `id` (`String`)、`name` (`String`)、`cost` (`i32`)。
- `logs`: 数组，最近系统/聊天事件日志，包含 `String`。

---

## 6. 标准 CAD 命令行分发规范

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
  (net-send! cid "Welcome to Minimal RPG!")
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
确保新增的 Scheme 代码未破坏宿主引擎的 19 项单元与集成测试。
