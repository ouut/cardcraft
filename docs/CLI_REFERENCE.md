# CardCraft CLI Reference & Multi-Server Guide

CardCraft 提供强大的多模式运行能力：支持原生终端 TUI、Web/WebSocket 浏览器连接、裸 TCP 自动化行协议、以及基于 Iroh 的 P2P 穿透联机。

---

## 1. 命令行参数概览

| 选项 / 参数 | 简写 | 默认值 | 描述 |
| :--- | :--- | :--- | :--- |
| `-g, --game <DIR>` | `-g` | `.` | 指定游戏根目录。自动加载 `<DIR>/core.scm` 或 `<DIR>/main.scm`，并将持久化文件沙盒隔离在 `<DIR>/saves/` |
| `--ws [PORT]` | 无 | `8080` | 启动 Web/WebSocket 混合服务（单端口同时提供静态 HTTP 页面与 WebSocket 实时双向流） |
| `--serve, --web-dir <DIR>` | 无 | `./web` | 指定静态网页文件托管目录（与 `--ws` 联动） |
| `--tcp [PORT]` | 无 | `9000` | 启动裸 TCP 纯文本行式交互服务（供 `nc`、`telnet`、Python 自动化脚本等接入） |
| `host` | 无 | - | 启动 P2P Host 模式，生成并打印可跨公网 NAT 穿透的连接 Ticket |
| `join <TICKET>` | 无 | - | 启动 P2P Join 模式，通过 Ticket 穿透连接到远端 Host |
| `--headless` | 无 | - | 无界面模式（不启动 Ratatui 终端界面），适合后台服务器或批处理运行 |
| `--test` | 无 | - | 运行内部集成自测（包含 Scheme 算力与状态机测试）并退出 |
| `-h, --help` | `-h` | - | 打印帮助信息 |
| `-v, --version` | `-v` | - | 打印版本信息 |

---

## 2. 详细选项说明与使用模式

### 2.1 游戏目录与沙盒隔离 (`-g, --game`)

CardCraft 允许每个游戏拥有独立的 Scheme 业务脚本和专属的存档空间。

```bash
# 运行指定目录下的游戏
cardcraft -g ./games/solitaire

# 启动 web/ws 服务运行自定义游戏
cardcraft -g ./games/market_rpg --ws 8080
```

#### 脚本解析优先级：
1. `<game_dir>/core.scm`
2. `<game_dir>/main.scm`
3. CardCraft 内置的默认《堆叠大陆/CardCraft》核心 Scheme 脚本。

#### 存档沙盒保护（Path Traversal Jail）：
- 游戏脚本调用 `(system-file-write "save.json" content)` 或 `(system-file-read "save.json")` 时，路径强制限定在 `<game_dir>/saves/` 目录下。
- 自动创建 `<game_dir>/saves/` 目录。
- 拒绝任何路径穿越攻击（例如 `../`, `../../etc/passwd` 会直接抛出安全异常）。

---

### 2.2 Web / WebSocket 混合服务 (`--ws`, `--serve`)

`--ws` 端口采用了**协议嗅探与单端口复用**技术（Single-Port HTTP + WS Multiplexing）：
- **普通 HTTP 请求**：返回 `--serve` 指定目录下的静态资源（默认 `./web`）。访问 `http://localhost:8080` 即可加载极简复古终端 Web 客户端。
- **WebSocket 升级握手**：若请求头包含 `Upgrade: websocket`，自动升级为全双工 WebSocket 长连接，与 Scheme 引擎进行双向事件与状态传输。

```bash
# 启动默认 Web 服务 (端口 8080, 静态根目录 ./web)
cardcraft --ws

# 指定端口并托管自定义 Web 前端
cardcraft --ws 3000 --serve ./my-custom-frontend

# 后台服务器模式运行 Web 端
cardcraft -g ./games/my_game --ws 8080 --headless
```

#### 浏览器访问：
打开浏览器访问 `http://localhost:8080/`：
- 页面自动建立 WebSocket 握手。
- 实时渲染服务端推送的 80x25 虚拟终端快照与聊天室消息。
- 支持在浏览器底部输入框直接输入 CAD 指令（如 `BUY basic`、`STACK 1 2`、`WHISPER 0 hello`）。

---

### 2.3 裸 TCP 纯文本服务端 (`--tcp`)

`--tcp` 服务端为各类外部工具、AI Agent、自动化测试以及终端工具提供了极简的以换行符 `\n` 分隔的 ASCII 接口。

```bash
# 启动 TCP 端口 9000
cardcraft --tcp 9000

# 同时开启 WebSocket (8080) 与 TCP (9000)
cardcraft --ws 8080 --tcp 9000
```

#### 客户端连接方式：
- **Netcat (`nc`)**:
  ```bash
  nc localhost 9000
  # 连接后会收到欢迎横幅与客户端 ID，如:
  # Welcome to CardCraft TCP Server! Your client ID is 0.
  
  # 发送 CAD 指令
  BUY basic
  # 发送广播消息
  CHAT Hello everyone!
  # 查看状态快照
  SNAPSHOT
  ```
- **Telnet**:
  ```bash
  telnet localhost 9000
  ```
- **Python / 自动化脚本**:
  ```python
  import socket
  s = socket.create_connection(('localhost', 9000))
  print(s.recv(1024).decode())
  s.sendall(b"BUY basic\n")
  print(s.recv(1024).decode())
  ```

---

### 2.4 Iroh P2P 穿透联机 (`host` / `join`)

基于 Iroh QUIC 协议栈，提供无需公网 IP 和端口映射的 P2P 直连联机对战。

```bash
# 主机端启动
cardcraft host
# 终端将输出生成的连接 Ticket:
# > Ticket: cardcraft-ticket://...

# 客户端加入
cardcraft join cardcraft-ticket://...
```

---

### 2.5 混合服务组合示例

CardCraft 允许同时启用多种网络接口，所有接入的客户端均进入同一个游戏引擎实例中协同或对战：

```bash
# 启动游戏 solitaire，同时监听 HTTP/WS (8080) 和 TCP (9000)，无界面后台运行
cardcraft -g ./games/solitaire --ws 8080 --tcp 9000 --headless
```

---

## 3. 并发架构与 Actor 线程模型

CardCraft 的 Steel Scheme 解释引擎包含非线程安全的引用计数类型 (`!Send`)。为了同时安全支撑异步多客户端网络通信（WebSocket、TCP、P2P）：

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
          /                 |                 \
         v                  v                  v
+----------------+  +----------------+  +----------------+
|  Tokio Task    |  |  Tokio Task    |  |  Tokio Task    |
|   WebSocket    |  |   TCP Socket   |  |   Iroh P2P     |
|   Clients      |  |   Clients      |  |   Peers        |
+----------------+  +----------------+  +----------------+
```

1. **`GameEngine` 驻留主 Actor 线程**：所有 Scheme 脚本求值、CAD 指令解析、状态变更、网络钩子触发 (`net-on-connect!`, `net-on-message!`, `net-on-disconnect!`) 均在单线程顺序执行，确保游戏状态的一致性与无锁安全。
2. **`EngineHandle` 跨线程分发**：网络连接仅持有轻量级可克隆的 `EngineHandle`，通过异步通道发送 `EngineCommand`，并将结果回传给网络套接字。

---

## 4. 常见问题 (FAQ)

### Q1: 客户端需要下载或执行 Scheme 脚本吗？
**不需要**。CardCraft 采用权威服务端（Authoritative Server）架构。客户端只需发送简短的 CAD 命令或文本消息，并接收服务器广播的状态快照或通知。

### Q2: 静态网页服务是否支持多 MIME 类型？
**是的**。内置的极简文件服务器自动为 `.html`, `.css`, `.js`, `.json`, `.svg`, `.png`, `.txt` 等设置正确的 `Content-Type`。

### Q3: 端口冲突或无法启动？
检查端口占用情况，或在命令行通过参数换用备用端口，例如 `--ws 8088 --tcp 9009`。
