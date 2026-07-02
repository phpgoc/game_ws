# game_ws

`ws` 是可开源的 WebSocket 游戏服务端目录。

## 目录

- `share_type_public/`: 公共协议类型，用于 web / ws / Android。
- `rust/common/`: Rust WS 公共库，包含房间、连接、运行时。
- `rust/landlord/`: 斗地主 Rust 服务端。
- `rust/shenyang_mahjong/`: 沈阳麻将 Rust 服务端。
- `rust/holdem/`: Hold'em 系列 Rust 服务端，承载德州、明牌德州、短牌德州和奥马哈。
- `android/`: 通用 Android 前台服务壳，当前使用 NDK 运行 `rust/landlord`。

## 依赖

基础 Rust：

```sh
rustup toolchain install stable
rustup default stable
rustup component add rustfmt
```

Android 额外需要：

```sh
cargo install cargo-ndk
rustup target add aarch64-linux-android x86_64-linux-android
```

并在 Android Studio SDK Manager 中安装 Android NDK。

## 运行 Rust WS 服务

在项目根目录运行更方便：

```sh
cargo run -p landlord -- --host 0.0.0.0 --port 9001
cargo run -p shenyang_mahjong -- --host 0.0.0.0 --port 9002
cargo run -p holdem -- --host 0.0.0.0 --port 9003
```

也可以在本目录运行：

```sh
cd ws
cargo run -p landlord -- --host 0.0.0.0 --port 9001
```

参数：

- `--host 0.0.0.0`: 局域网可访问。
- `--host 127.0.0.1`: 只允许本机访问。
- `--port`: 指定端口。

## 检查和测试

```sh
cargo check -p landlord
cargo test -p landlord
cargo check -p shenyang_mahjong -p holdem
```

## 运行 Android 斗地主服务

Android 目录：

```sh
cd ws/android
```

模拟器：

```sh
./gradlew --no-daemon :app:assembleDebug -PrustAbis=x86_64
```

真机：

```sh
./gradlew --no-daemon :app:assembleDebug -PrustAbis=arm64-v8a
```

默认同时构建 `arm64-v8a` 和 `x86_64`：

```sh
./gradlew --no-daemon :app:assembleDebug
```

## 网络配置

服务使用纯 WS 协议。生产环境如果需要 WSS，可以用 Nginx 反向代理：

```nginx
upstream game_ws {
    server localhost:9001;
}

server {
    listen 443 ssl;
    server_name your.domain.com;

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/cert.key;

    location / {
        proxy_pass http://game_ws;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```
