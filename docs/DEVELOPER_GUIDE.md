# CardCraft 玩家与开发者指南 (Human Guide)

本文档面向人类玩家与开发者，涵盖 **CLI 启动参数、Stacklands 核心玩法与配方、系统架构设计、网络流机制以及二次开发指南**。

---

## 一、 快速启动与 CLI 参数 (`src/main.rs`)

### 1. 默认全开服务模式
不带参数运行 `cardcraft` 时，程序默认在一个进程内并发启动 **本地 TUI 终端 + HTTP/WebSocket 服务 (:8080) + 裸 TCP 服务 (:9000) + Iroh P2P 节点**：

```bash
# 启动默认 Stacklands 游戏
cargo run --release

# 加载指定目录的独立游戏（读取 <DIR>/core.scm，托管 <DIR>/web/，存档隔离在 <DIR>/saves/）
cargo run --release -- -g examples/chatroom
```

### 2. 加入好友的 P2P 对局
房主启动后会生成一段 `cardcraft://...` 加密节点票据（Ticket）。客方直接运行 `join` 即可通过 Iroh QUIC 自动打洞直连，并在本地同步桥接出 TUI、`:8080` 网页端与 `:9000` TCP 端口：

```bash
cargo run --release -- join cardcraft://7b22696422...
```

### 3. 命令行参数一览

| 参数 | 说明 | 默认值 |
| :--- | :--- | :--- |
| `-g, --game <DIR>` | 指定游戏目录（加载 `<DIR>/core.scm` 或 `main.scm`） | `.` |
| `--serve, --web-dir <DIR>` | 覆盖静态网页托管目录 | `<game>/web` 或 `./web` |
| `--ws, --ws-port <PORT>` | 设置 HTTP + WebSocket 复用端口 | `8080` |
| `--tcp, --tcp-port <PORT>` | 设置裸 TCP 行协议端口（可用 `nc` 连接） | `9000` |
| `--headless, --no-tui` | 关闭本地终端界面，作为纯后台无头服务器运行 | 关闭 |
| `--no-net` | 纯单机离线模式（关闭 WS、TCP 与 P2P） | 关闭 |
| `--no-ws` / `--no-tcp` / `--no-p2p` | 单独关闭对应网络服务 | 关闭 |
| `--test` | 运行内置无头自检测试并输出虚拟终端渲染帧 | - |
| `-h, --help` / `-v, --version` | 查看帮助或版本信息 | - |

---

## 二、 游戏指令与合成配方 (`scripts/core.scm`)

### 1. CAD 命令行与多人指令
无论在本地 TUI、浏览器底部输入框，还是 `nc 127.0.0.1 9000` 中，均使用统一的指令集：

| 类别 | 指令 | 示例 | 作用 |
| :--- | :--- | :--- | :--- |
| **卡牌操作** | `s <from> <to>` | `s 1 2` | 将卡栈 `from` 叠到 `to` 上，自动匹配合成配方 |
| **卡牌操作** | `u <id>` | `u 2` | 从卡栈 `id` 顶部拆出一张卡到新卡槽 |
| **卡牌操作** | `m <from> <to>` | `m 3 10` | 将卡栈 `from` 移动到空闲槽位 `to` |
| **商店与经济** | `b <pack>` | `b basic` | 购买卡包：`basic`(3G), `food`(5G), `struct`(10G), `explore`(12G) |
| **商店与经济** | `o <id>` | `o 6` | 打开槽位 `id` 中的卡包，随机获得 3 张卡牌 |
| **商店与经济** | `x <id>` | `x 4` | 出售槽位 `id` 顶部的卡牌换取金币 |
| **时间控制** | `w [秒数]` | `w 5` | 快进指定秒数（默认 1 秒） |
| **时间控制** | `p` | `p` | 暂停 / 恢复游戏时间推进 |
| **多人社交** | `nick <name>` | `nick Alice` | 修改自己的在线昵称 |
| **多人社交** | `say <msg>` | `say Hello` | 发送全服公屏消息 |
| **多人社交** | `w <user> <msg>` | `w Bob Hi!` | 向指定昵称的玩家发送私聊密语（目标非数字时识别为私聊） |
| **多人社交** | `who` | `who` | 查看当前在线玩家列表 |
| **上帝模式** | `(<scheme>)` | `(spawn-card! "Sword")` | 本地控制台中以 `(` 开头的输入直接作为 Scheme 代码求值 |

### 2. 核心合成配方表（部分代表性配方）
* **采集**：`Villager + BerryBush` -> `Berry` (4s) | `Villager + Tree` -> `Wood` (5s) | `Villager + Rock` -> `Stone` (5s) | `Villager + IronDeposit` -> `IronOre` (6s)
* **加工**：`Villager + Wood` -> `Stick x2` (3s) | `Villager + Wood x2` -> `Plank` (4s) | `Villager + Stone x2` -> `Brick` (4s) | `Smelter + IronOre + Wood` -> `IronBar` (5s)
* **建筑**：`Villager + Wood x2 + Stone` -> `Campfire` / `LumberCamp` | `Villager + Wood + Stone x2` -> `Shed` (+5 卡牌上限) | `Villager + Wood x2 + Stone x2` -> `House` | `Villager + Plank + Brick + IronBar` -> `Warehouse` (+10 卡牌上限)
* **烹饪与农业**：`Campfire + Berry` -> `CookedBerry` | `Campfire + RawMeat` -> `CookedMeat` | `Villager + Soil + Berry` -> `BerryBush`
* **军事与繁衍**：`Villager + Wood + Flint` -> `Spear` | `Villager + Spear` -> `Militia` | `Villager + Sword` -> `Swordsman` | `Villager x2 + House` -> `Baby` -> `Villager`

---

## 三、 系统架构设计

```text
+-------------------------------------------------------------------------+
|                   GameEngine Actor (tokio::task::LocalSet)              |
|            持有唯一 Steel Scheme VM 实例 (Single Source of Truth)         |
+-------------------------------------------------------------------------+
               ^                                           |
               | EngineCommand (Connect/Message/Tick)      | NetEvent (system-net-send!)
               |                                           v
+-------------------------------------------------------------------------+
|                        EngineHandle & ClientHub                         |
+-------------------------------------------------------------------------+
     /                      |                        |                 \
    v                       v                        v                  v
[Local TUI]          [Web/WS :8080]           [TCP :9000]         [Iroh P2P]
(Crossterm)       (HTTP 静态 + WS 升级)      (nc / 自动化脚本)    (QUIC 穿透桥接)
```

### 1. 窄腰沙漏架构 (Narrow-Waist Architecture)
* **Rust 宿主层 (`src/engine.rs`, `src/server.rs`, `src/p2p.rs`, `src/render.rs`)**：不存储任何游戏业务概念（无卡牌、金币或地图结构）。仅负责终端事件采集、网络并发 I/O、文件沙盒校验与 ANSI 字符网格渲染。
* **Scheme 逻辑层 (`scripts/core.scm`)**：100% 掌控游戏状态机、多重集配方匹配、行编辑器（`*tui-input-buffer*`）、全屏 ASCII/ANSI 排版（`game-render`）以及所有客户端的网络推送。

### 2. 纯流转发网络模型 (Pure `system-net-send!` Stream)
* Rust 的 TCP、WebSocket 与 P2P 服务端**不主动拉取或包装任何游戏快照**。
* 当客户端连入、发消息或时钟推进时，何时发数据、发给哪个 `client_id`、发普通文本还是 JSON 画面快照，**完全由 Scheme 调用 `(system-net-send! cid msg)` 决定**。
* 因此，如果是多人处在不同地图的 RPG 游戏，Scheme 只需根据每个 `cid` 所在的地图分别调用 `(system-net-send! cid (render-map-for cid))`，Rust 层无需任何改动。

### 3. 单端口 HTTP 与 WebSocket 协议嗅探 (`src/server.rs`)
* `run_web_server` 通过 `TcpStream::peek` 嗅探请求头：若包含 `upgrade: websocket` 则交由 `tokio-tungstenite` 升级为 WebSocket 流；否则作为静态文件 HTTP 服务器返回 `<web_dir>/index.html`。

---

## 四、 二次开发与测试指南

### 1. 在 Rust 中向 Scheme 注入新函数 (`src/engine.rs`)
在 `GameEngine::with_game_dir` 中通过 `vm.register_fn` 即可暴露新的 Rust 函数：

```rust
vm.register_fn("system-my-func", |arg: String| -> String {
    format!("Processed by Rust: {}", arg)
});
```

### 2. 编写新游戏或示例 (`examples/`)
只需新建一个目录（如 `examples/my_rpg/`），放入 `core.scm`（以及可选的专属 `web/index.html`），实现 `game-start!`、`game-update!`、`game-render` 和 `net-on-message!` 四个函数，即可通过 `cargo run -- -g examples/my_rpg` 运行。

### 3. 运行自动化测试 (`tests/headless_tests.rs`)
借助 `VirtualScreen`（基于 `ratatui::backend::TestBackend` 的内存屏幕），所有 UI 渲染与网络交互均可无头测试：

```bash
cargo test
```
