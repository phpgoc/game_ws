# game_ws

WS 子模块目录：

- `share_type_public/`：公共协议类型（可开源）
- `common/rust/`：Rust WS 公共库 + 启动入口（`main` 在这里）
- `landlord/rust/`：斗地主 WS 业务库（只放协议与游戏逻辑）

## 为什么不用 axum ws

这里选择 `tokio-tungstenite` 直连 `TcpListener`，并把启动流程收敛到 `common`：

- 抽象更少，可读性更直观
- 对游戏服连接生命周期控制更直接
- 更适合先做协议驱动的实时 server

## 启动

在 `ws` 目录运行：

```bash
cargo run -p ws_common -- --host 192.168.1.10 --port 9001
```

参数规则：

- `--host` 可选：不传时自动选择私网 IPv4（10.x / 172.16-31.x / 192.168.x）
- `--port` 可选：不传时自动选择大于 9000 的可用端口
- `host/port` 不合法时，进程直接退出
