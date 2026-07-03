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

Ubuntu / Debian 构建公版 Rust WS 服务端：

```sh
sudo apt update
sudo apt install -y build-essential pkg-config
```

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

## 发布 Rust WS 服务端

公版自建 WS 服务端不包含 official 统计和 SQLite；官方服需要统计时再使用带 `data`/SQLite 的构建或独立服务。面向普通 Linux 用户发布时，优先使用 musl 目标构建静态单文件。

推荐在 Linux、WSL 或 Linux CI 上构建 Linux musl release。`rustup target add` 只安装目标标准库，不会自动安装跨平台 linker；Windows/macOS 原生交叉编译路径不作为维护重点，优先使用发布产物或 Linux/WSL/CI 构建。

Linux / WSL 准备发布环境：

```sh
sudo apt install -y musl-tools
rustup target add x86_64-unknown-linux-musl
```

构建发布包：

```sh
cargo build --release --target x86_64-unknown-linux-musl \
  -p landlord \
  -p shenyang_mahjong \
  -p texas_hold_em \
  -p tractor
```

产物位置：

```sh
target/x86_64-unknown-linux-musl/release/landlord
target/x86_64-unknown-linux-musl/release/shenyang_mahjong
target/x86_64-unknown-linux-musl/release/texas_hold_em
target/x86_64-unknown-linux-musl/release/tractor
```

WSL Ubuntu 26.04 已验证这些产物为 `static-pie linked`，`ldd` 显示 `statically linked`。当前 musl 构建只有一个已知提示：`landlord` 的 lib 目标同时声明了 `cdylib`，musl 目标会提示 `dropping unsupported crate type cdylib`。这不影响服务端二进制发布；如果以后想消除提示，可以把 Android/JNI 用的 `cdylib` 和 Linux server bin 的 crate-type 做 feature 或包边界拆分。

除了 musl，也可以发布 glibc 动态链接二进制，或者在 Docker/OCI 镜像里发布服务端。glibc 方案体积和调试体验更接近普通 Linux，但会受发行版 glibc 版本影响；Docker 方案部署一致性好，但不满足“直接下载一个文件运行”的目标。

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
