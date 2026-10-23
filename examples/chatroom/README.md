# CardCraft 示例：极简多用户终端聊天室 (Terminal Chatroom)

基于 CardCraft 引擎打造的纯 Scheme 多人实时聊天室。**未修改任何 Rust 代码**，100% 由 Scheme 驱动全屏字符画布、网络业务路由与多用户在线状态。

---

## 🌟 启动与运行

```bash
# 进入仓库根目录，启动聊天室
cargo run --release -- -g examples/chatroom
```

* **本地终端 Host**：直接进入全屏终端界面，打字按 Enter 发送消息。
* **网页浏览器**：浏览器打开 `http://localhost:8080` 即时加入聊天。
* **TCP 命令行玩家**：在另一终端输入 `nc localhost 9000` 即可实时收发消息。
* **P2P 联机好友**：复制终端输出的 `cardcraft://...` Ticket，好友输入 `cardcraft join <ticket>` 即可跨公网穿透加入！

---

## 💬 聊天室指令速查

| 指令 | 说明 | 示例 |
| :--- | :--- | :--- |
| `<任意文本>` | 直接在公屏发送消息 | `Hello everyone!` |
| `/nick <新昵称>` | 修改自己的聊天昵称 | `/nick Alice` |
| `/w <用户名> <内容>` | 发送私人密语（或 `/tell`） | `/w Bob 只有你能看到` |
| `/who` | 查看当前所有在线用户列表 | `/who` |
| `/clear` | 清空屏幕消息历史 | `/clear` |
| `/help` | 显示指令帮助菜单 | `/help` |
