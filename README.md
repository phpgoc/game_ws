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
cargo run -p tractor -- --host 0.0.0.0 --port 9004
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
cargo check -p shenyang_mahjong -p holdem -p tractor
cargo test -p tractor
```

拖拉机房间开始后会锁定设置。当前主要设置包括：`deck_count`（几副牌）、`removed_rank_count`（按 `3/4/6/7/8/9/J/Q/A` 的顺序删掉前 N 个点数，`0` 表示不删）、`first_deal_time`（首局发牌总时间，毫秒）、`deal_time`（后续局发牌总时间，毫秒）、`target_rank`（最终目标 rank）、`blood_enabled` / `blood_start_score` / `blood_score_per_unit`（喝血相关）。拖拉机采用逐张发牌，发牌过程中可亮主/反主；发完后庄家收底并扣回相同张数，随后进入出牌。

## 发布 Rust WS 服务端

公版自建 WS 服务端不包含 official 统计和 SQLite；官方服需要统计时再使用带 `data`/SQLite 的构建或独立服务。

推荐下载 Linux x86_64 musl release 产物。该产物是静态单文件，适合大多数 x86_64 Linux 服务器直接运行。

### Linux x86_64 musl

从源码构建 Linux x86_64 musl release：

```sh
sudo apt install -y musl-tools
rustup target add x86_64-unknown-linux-musl
```

构建发布包：

```sh
cargo build --release --target x86_64-unknown-linux-musl \
  -p landlord \
  -p shenyang_mahjong \
  -p holdem \
  -p tractor
```

产物位置：

```sh
target/x86_64-unknown-linux-musl/release/landlord
target/x86_64-unknown-linux-musl/release/shenyang_mahjong
target/x86_64-unknown-linux-musl/release/holdem
target/x86_64-unknown-linux-musl/release/tractor
```

构建时如果看到 `dropping unsupported crate type cdylib`，可以忽略。服务端二进制仍会正常生成。

### macOS 交叉编译 Linux musl

macOS 上交叉编译到 Linux musl 需要额外安装 linker。可以用 Homebrew 安装 [`FiloSottile/musl-cross`](https://github.com/FiloSottile/homebrew-musl-cross)：

```sh
brew install FiloSottile/musl-cross/musl-cross
rustup target add x86_64-unknown-linux-musl
```

确认 linker 已经在 PATH 中：

```sh
which x86_64-linux-musl-gcc
which x86_64-linux-musl-cc
```

临时指定 linker 构建：

```sh
CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=x86_64-linux-musl-gcc \
cargo build --release --target x86_64-unknown-linux-musl \
  -p landlord \
  -p shenyang_mahjong \
  -p holdem \
  -p tractor
```

也可以写进本机 `~/.cargo/config.toml`，这样以后不用每次传环境变量：

```toml
[target.x86_64-unknown-linux-musl]
linker = "x86_64-linux-musl-gcc"
```

### Windows 静态 release（不推荐运行环境）

Windows 不是推荐运行环境。WS 服务端即使能运行，也要额外考虑 Windows 防火墙、局域网发现、杀毒软件、端口开放和执行策略等问题。Windows release 仅用于验证或自行构建；公开发布页优先提供 Linux musl 静态产物。

如需验证 Windows release，可以在 Windows PowerShell 中静态链接 MSVC CRT：

```powershell
rustup target add x86_64-pc-windows-msvc
$env:RUSTFLAGS="-C target-feature=+crt-static"
cargo build --release --target x86_64-pc-windows-msvc `
  -p landlord `
  -p shenyang_mahjong `
  -p holdem `
  -p tractor
Remove-Item Env:RUSTFLAGS
```

产物位置：

```powershell
target\x86_64-pc-windows-msvc\release\landlord.exe
target\x86_64-pc-windows-msvc\release\shenyang_mahjong.exe
target\x86_64-pc-windows-msvc\release\holdem.exe
target\x86_64-pc-windows-msvc\release\tractor.exe
```

### 维护约束

维护 release 脚本、CI 或自动生成的构建说明时，保持以下约束：

```text
推荐 release 产物：Linux x86_64 musl 静态单文件。
release 包范围：landlord、shenyang_mahjong、holdem、tractor。
Windows 不作为推荐运行环境；如需 Windows 构建说明，只保留 x86_64-pc-windows-msvc + crt-static 的验证命令，并提醒防火墙、杀毒软件、端口开放和执行策略需要额外处理。
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
