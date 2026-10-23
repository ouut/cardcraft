# CardCraft CLI Reference & Multi-Server Guide

CardCraft 提供强大的多模式运行能力：默认全开本地终端 TUI、Web/WebSocket 浏览器连接、裸 TCP 自动化行协议、以及基于 Iroh 的 P2P 穿透联机。

---

## 1. 命令行参数概览

| 选项 / 参数 | 简写 | 默认行为 | 描述 |
| :--- | :--- | :--- | :--- |
| **(无参数)** | - | **默认全开** | 自动同时开启本地 TUI + Web(:8080) + TCP(:9000) + P2P Ticket 生成 |
| `-g, --game <DIR>` | `-g` | `.` | 指定游戏根目录。自动加载 `<DIR>/core.scm`，数据隔离在 `<DIR>/saves/`，静态页探测 `<DIR>/web/` |
| `--serve, --web-dir <DIR>` | 无 | `<game>/web` 或 `./web` | 手动覆盖静态网页文件托管目录 |
| `join <TICKET>` | 无 | - | 作为客机加入远端对局，本地同时拉起 TUI 终端与网页桥接 |
| `--ws [PORT]` | 无 | `8080` | 自定义 Web/WebSocket 服务端口 |
| `--tcp [PORT]` | 无 | `9000` | 自定义裸 TCP 纯文本行式交互服务端口 |
| `--headless`, `--no-tui` | 无 | - | 关闭本地 TUI 界面（纯后台服务器模式，适合云主机或 Docker 部署） |
| `--no-net` | 无 | - | 纯单机离线模式（完全关闭网络与所有端口占用） |
| `--no-ws` | 无 | - | 仅关闭 Web/WebSocket 服务 |
| `--no-tcp` | 无 | - | 仅关闭 TCP 服务 |
| `--no-p2p` | 无 | - | 仅关闭 P2P 穿透端点 |
| `--test` | 无 | - | 运行内部集成自测（包含 Scheme 算力与状态机测试）并退出 |
| `-h, --help` | `-h` | - | 打印帮助信息 |
| `-v, --version` | `-v` | - | 打印版本信息 |

---

## 2. 核心架构与各通道职责

### 2.1 游戏目录规范 (`-g, --game`)
每个游戏自成一个独立的“游戏资产包（Game Bundle）”，脚本、存档、网页各司其职：

```text
games/
└── my_rpg/
    ├── core.scm (或 main.scm)   # [Script] 100% 游戏业务规则、配方、数值与逻辑
    ├── saves/                   # [Save]   沙盒隔离持久化目录（存储 json/scm 存档）
    └── web/                     # [Web]    该游戏专用的网页客户端（HTML/CSS/JS 资源）
```

#### 资源加载与解析策略：
1. **脚本探测**：优先加载 `<game_dir>/core.scm` $\to$ `<game_dir>/main.scm` $\to$ 内置兜底脚本。
2. **沙盒根目录**：文件读写严格封锁在 `<game_dir>/saves/`，拒绝任何路径穿越（`../`）。
3. **Web 资源目录**：
   - 自动探测：若 `<game_dir>/web/` 存在，则 HTTP 服务直接托管该目录；
   - 备选探测：若当前游戏没有自己的 `web/`，则回退使用全局默认的 `./web/` 终端控制台网页；
   - 覆盖选项：通过 `--serve <DIR>` 可以手动指定任意外部前端工程目录。

---

### 2.2 协议接入分层：各通道定位分明

CardCraft 统一使用 `EngineHandle` 作为权威中枢通道，所有接入端完全平权：

| 接入方式 | 协议载体 | 定位与目标客户端 | 交互特点 |
| :--- | :--- | :--- | :--- |
| **本地 TUI** | 内存通道 (`EngineHandle`) | 本机控制台（极客/无桌面环境/SSH） | 80x25 字符画渲染、键盘快捷键输入 |
| **WS 方式** | HTTP 静态托管 + WebSocket | 浏览器现代富前端（Pixi.js / Vue / 拖拽） | 端口默认 `8080`，双向传输 JSON 数据快照 |
| **TCP 方式** | 裸 TCP Socket（行协议） | 远端终端直连、CI 自动化脚本、AI Agent | 端口默认 `9000`，`\n` 分隔的 ASCII 命令与反馈 |
| **P2P 管道** | Iroh QUIC 加密隧道 | 跨公网、免中央服务器的穿透传输通道 | 本身不绑定应用层业务，承载 TCP 或 WS 流 |

---

## 3. 详细使用场景示例

### 3.1 零配置开箱即用（全服务并发就绪）
```bash
# 启动默认游戏
cardcraft

# 启动指定游戏
cardcraft -g ./games/solitaire
```
* **本地控制台**：立即进入 TUI 终端游戏界面。
* **浏览器访问**：打开 `http://localhost:8080`，即可通过网页界面同步游玩！
* **远端脚本**：使用 `nc localhost 9000` 直连交互。
* **联机好友**：终端启动日志中已包含生成的 Iroh P2P Ticket，朋友输入 Ticket 即可穿透连入。

### 3.2 加入他人对局 (`join`)
```bash
# 客机通过 Ticket 连入好友对局
cardcraft join cardcraft://...
```
* 本地自动作为 P2P 客户端连接远端主机。
* 本地同时启动本地 TUI 界面与 `http://localhost:8080` 网页桥接，玩家既可以在终端玩，也可以在浏览器玩。

### 3.3 无头后台服务器模式 (`--headless` / `--no-tui`)
适合部署在 Linux 云主机、Docker 容器或 CI 环境中：
```bash
cardcraft -g ./games/market_rpg --headless
```
* 控制台不占用终端屏幕，打印出 Web 端口与 P2P Ticket 供外部连接。

### 3.4 纯单机离线模式 (`--no-net`)
完全不占用任何网络端口，零安全风险：
```bash
cardcraft --no-net
```

---

## 4. 并发架构与 Actor 线程模型

CardCraft 的 Steel Scheme 解释引擎包含非线程安全的引用计数类型 (`!Send`)。为了同时安全支撑本地 TUI、WebSocket、TCP 与 P2P 多方并发操作：

```
+-------------------------------------------------------+
|                 GameEngine Actor Thread               |
|            (Holds Single GameEngine Instance)         |
+-------------------------------------------------------+
                           ^
                           | mpsc (EngineCommand)
                           v
        +---------------------------------------+
        |         EngineHandle (Send + Sync)    |
        +---------------------------------------+
          /          |              |          \
         v           v              v           v
+----------------+ +------------+ +------------+ +------------+
|  Local TUI     | | WebSocket  | | TCP Socket | |  Iroh P2P  |
|  Terminal Loop | |  (Port)    | |  (Port)    | |   Peers    |
+----------------+ +------------+ +------------+ +------------+
```

1. **`GameEngine` 驻留主 Actor 线程**：所有 Scheme 脚本求值、CAD 指令解析、状态变更、网络钩子触发 (`net-on-connect!`, `net-on-message!`, `net-on-disconnect!`) 均在单线程顺序执行，确保游戏状态的一致性与无锁安全。
2. **`EngineHandle` 跨线程分发**：本地 TUI、网络套接字与 P2P 桥接均持有轻量级可克隆的 `EngineHandle`。
3. **双向协同 (Co-op)**：本地玩家在终端输入指令，网页端瞬间更新；网页端玩家拖拽卡牌，本地终端实时重绘。

---

## 5. 常见问题 (FAQ)

### Q1: 客户端需要下载或执行 Scheme 脚本吗？
**不需要**。CardCraft 采用权威服务端（Authoritative Server）架构。客户端只需发送简短的 CAD 命令或文本消息，并接收服务器广播的状态快照或通知。

### Q2: 静态网页服务是否支持多 MIME 类型？
**是的**。内置的极简文件服务器自动为 `.html`, `.css`, `.js`, `.json`, `.svg`, `.png`, `.txt` 等设置正确的 `Content-Type`。

### Q3: 端口冲突或无法启动？
可以通过 `--ws 8088 --tcp 9009` 自定义端口，或者使用 `--no-ws` / `--no-tcp` 停用特定服务。
