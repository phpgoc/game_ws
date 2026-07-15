# 通用两人 P2P 服务

该 crate 不依赖 `ws_common` 或任何游戏 crate，只使用 `share_type_public` 中的通用
P2P 协议。单个 Rust 进程同时提供：

- 两人房间和 WebRTC SDP/ICE WebSocket 信令；
- 内置 STUN Binding 服务；
- 内置带短期 HMAC 凭证的 UDP TURN relay；
- 按 `game + room` 隔离房间，游戏数据不经过信令服务。

STUN/TURN 使用纯 Rust `turn` crate，不需要安装或启动 coturn。这种结构也便于 Android
通过 Kotlin 前台服务和 NDK 启动同一个 Rust runtime。

## 运行

至少设置一个足够长的随机密钥：

```sh
export P2P_TURN_SECRET='replace-with-at-least-16-random-bytes'
cargo run -p p2p -- --host 0.0.0.0 --port 9005
```

默认监听：

- TCP `9005`：WebSocket 信令；
- UDP `3478`：同一个端口上的 STUN 与 TURN；
- UDP `49160-49200`：TURN relay 分配端口。

可选环境变量：

```sh
export P2P_TURN_PUBLIC_IP='203.0.113.10'
export P2P_TURN_BIND_IP='0.0.0.0'
export P2P_TURN_RELAY_BIND_IP='0.0.0.0'
export P2P_TURN_PORT=3478
export P2P_TURN_RELAY_MIN_PORT=49160
export P2P_TURN_RELAY_MAX_PORT=49200
export P2P_TURN_REALM='lan-game-p2p'
export P2P_TURN_TTL_SECONDS=3600
```

`P2P_TURN_PUBLIC_IP` 未设置时会自动选择默认路由的本机 IPv4，适合局域网开服。公网
NAT 部署必须显式填写公网 IP，并将 UDP 3478 和 relay 端口范围全部映射到服务器。

HTTPS 页面连接信令时仍需使用 `wss://`。当前内置 TURN 支持 UDP；WebRTC DataChannel
在其上运行 DTLS/SCTP，仍然是可靠、有序、带重传的消息通道，并不是裸 UDP 游戏协议。

## Android / Kotlin

NDK 调用与 App 使用相同进程和 UID。Manifest 需要 `android.permission.INTERNET`；UDP
3478 不需要 root 或运行时权限。长期开服应使用 Android 前台服务，并根据需要持有
Wi-Fi lock/唤醒锁。蜂窝网络常有 CGNAT，因此“监听成功”不表示公网一定可达。

TURN 不能匿名开放。服务默认只接受信令服务签发的短期凭证，并限制 relay 端口范围；
部署者仍应在系统防火墙中只开放信令、3478 和指定 relay 范围。
