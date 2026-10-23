# CardCraft (控制台字符版 Stacklands 建造模拟)

> **“一切皆卡牌，堆叠即生产”** —— 基于 Rust + Steel Scheme 打造的极简字符控制台即时生存与建造模拟游戏。

---

## 🌟 项目亮点

1. **极简字符美学（Row-as-Stack）**：
   * 终端每行代表一个独立的卡栈，卡牌用具有色彩高亮的单词表示（如 `Villager`, `BerryBush`, `Wood`）。
2. **CAD 风格命令行交互（CAD-Style CLI）**：
   * 采用类似 AutoCAD 的动词缩写与目标索引指令（`s 1 2` 堆叠、`u 2` 拆分、`b basic` 购包、`o 3` 拆包）。
   * 支持命令历史回溯（上下方向键）、行内光标移动编辑，支持 `(Scheme 代码)` 原生无缝求值。
3. **双核沙漏解耦架构（Rust + Steel Scheme）**：
   * **Host（Rust）**：负责跨平台终端控制（Crossterm）、统一硬件事件流（按键、鼠标、窗口重绘）直通转发与纯粹的全屏字符网格投影。
   * **Guest（Steel Scheme）**：100% 掌管游戏世界状态、全屏字符网格排版渲染、行编辑与命令状态机、列表多重集模式匹配、合成进度与月度生存结算。
4. **Headless 虚拟缓冲层（Decoupled Virtual Buffer）**：
   * 渲染层通过纯内存虚拟缓冲抽象（基于 Ratatui），零物理屏幕依赖即可进行百分之百像素级自动化测试与跨端投影（WebAssembly / 终端 / CI 单测）。

---

## 🚀 快速启动

### 前置环境
* [Rust](https://rustup.rs/) (1.75+ 推荐)
* Cargo 包管理器

### 1. 启动游戏（默认全开模式）
```bash
# 直接运行：默认同时开启 TUI 终端 + Web(8080) + TCP(9000) + P2P Ticket
cargo run --release

# 运行指定游戏目录（自动加载 <dir>/core.scm, 隔离 <dir>/saves/, 托管 <dir>/web/）
cargo run --release -- -g ./games/my_game
```
* **本地玩家**：在终端通过字符界面和底部 CAD 命令行直接操作。
* **浏览器玩家**：打开 `http://localhost:8080`，通过 Web 界面同步操作同一局游戏。
* **远端脚本/AI**：通过 `nc localhost 9000` 发送指令。
* **联机好友**：拿到 P2P Ticket 即可直接加入。

### 2. 加入好友对局 (P2P 联机)
```bash
# 作为客户端加入（本地同时拉起 TUI 终端与 http://localhost:8080 网页桥接）
cargo run --release -- join cardcraft://...
```

### 3. 按需微调与无头服务模式 (Opt-out)
```bash
# 无头后台服务器模式（关闭 TUI，保留 Web/TCP/P2P，适合云主机或 Docker）
cargo run --release -- --headless

# 纯单机离线模式（完全关闭网络与端口占用，纯本地终端极速游玩）
cargo run --release -- --no-net

# 自定义端口覆盖
cargo run --release -- --ws 3000 --tcp 9999
```

### 4. 运行全套自检与测试
```bash
# 运行内置无头自检，并在控制台打印虚拟渲染帧
cargo run --release -- --test

# 运行全部 24 项端到端单元与集成测试
cargo test
```

---

## 🎮 玩法指引（3分钟上手）

### 初始局面
游戏开局提供 5 个卡槽：
* `[ 1] Villager` （村民，核心劳动力）
* `[ 2] BerryBush` （浆果丛，食物来源）
* `[ 3] Berry` （浆果，饱食度 1）
* `[ 4] Wood` （木头，建筑材料）
* `[ 5] Coin` （金币，通用货币）

### 基础合成链
1. **采集食物**：
   输入 `s 1 2`，将村民压在浆果丛上。卡栈上方会出现进度条 `[====....] (Harvest Berry)`。4秒倒计时结束后，将在空闲槽位自动弹出一颗新 `Berry`。
2. **拆分卡牌**：
   如果想调离村民，输入 `u 2`，村民将自动出栈并移至新行。
3. **购买与开启卡包**：
   输入 `b basic` 消耗 3G 购买基础包；输入 `o <槽位ID>` 开启卡包，爆出木头、石头与浆果。
4. **周期生存（Moon Cycle）**：
   每 30 秒为一个“月度（Moon）”。月末会按村民人数扣除场上的食物（浆果/熟食）。食物不足时村民会饿死！

---

## ⌨️ CAD 命令行速查表

| 指令 | 含义 | 示例 | 说明 |
| :--- | :--- | :--- | :--- |
| **`s <源> <目标>`** | Stack（堆叠） | `s 1 2` | 将槽位 1 的卡全部压在槽位 2 上，尝试触发配方 |
| **`u <槽位>`** | Unstack（出栈） | `u 2` | 从指定槽位弹出一张卡，自动开辟新槽位放置 |
| **`m <源> <目标>`** | Move（移动） | `m 3 10` | 将槽位 3 整体移动到空闲槽位 10 |
| **`b <包名>`** | Buy（购买卡包） | `b basic` | 购买卡包（`basic` 3G, `food` 5G） |
| **`o <槽位>`** | Open（开启卡包）| `o 6` | 拆开卡包，随机爆出 3 张资源卡 |
| **`x <槽位>`** | Sell（出售卡牌）| `x 4` | 卖出槽位顶部的卡牌换取金币 |
| **`w [秒数]`** | Wait（快速等待）| `w 5` | 推进时间流逝指定秒数（快进合成或结算） |
| **`p`** | Pause（暂停） | `p` | 暂停或恢复时间流动 |
| **`h`** | Help（帮助） | `h` | 显示命令提示 |
| **`q`** | Quit（退出） | `q` | 退出游戏 |
| **`(...)`** | Lisp God Mode | `(spawn-card! "Apple")` | 输入括号开头的文本直接作为 Scheme 动态求值！ |

---

## 📚 文档导航

* 📖 **[玩家与开发者指南 (docs/DEVELOPER_GUIDE.md)](docs/DEVELOPER_GUIDE.md)**：面向人类读者的完整指南，涵盖 CLI 参数、Stacklands 合成配方表、系统架构、纯流网络模型与二次开发说明。
* 🤖 **[AI Agent 接口与引擎规范 (docs/AGENT_API_REFERENCE.md)](docs/AGENT_API_REFERENCE.md)**：面向 AI Agent 的精确技术规范，涵盖宿主注入原语签名、事件流格式、JSON 视图契约与多地图 RPG 推送模板。

---

## 📜 许可证

本项目采用 MIT 许可证。
