# game_ws

`ws` 是可开源的 WebSocket 游戏服务端目录。

## 目录

- `share_type_public/`: 公共协议类型，用于 web / ws / Android。
- `rust/common/`: Rust WS 公共库，包含房间、连接、运行时。
- `rust/landlord/`: 斗地主 Rust 服务端。
- `rust/shenyang_mahjong/`: 沈阳麻将 Rust 服务端。
- `rust/holdem/`: Hold'em 系列 Rust 服务端，承载德州、明牌德州、短牌德州和奥马哈。
- `rust/p2p/`: 独立的两人 WebRTC 信令服务与 STUN/TURN 临时凭证签发器，不依赖其他游戏 crate。
- `android/`: 5 个 Rust 服务共用的 Android 前台服务壳，每个 APK 只打包对应的 `.so`。

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

## 统一构建 10 个产物

`build_script/build_all.sh` 会一次生成 5 个 Linux x86_64 musl 可执行文件和
5 个 Android APK：

```sh
./build_script/build_all.sh
```

macOS 首次构建可先安装全部依赖：

```sh
./build_script/install_deps_mac.sh
```

不想在主机安装工具链时使用 Docker：

```sh
./build_script/build_in_docker.sh
```

10 个文件统一输出到 `build_script/output/`，名称分别为
`landlord` / `shenyang_mahjong` / `holdem` / `tractor` / `p2p` 及其同名 `.apk`。

## 运行 Rust WS 服务

在公版仓库根目录运行：

```sh
cargo run --manifest-path rust/landlord/Cargo.toml -- --host 0.0.0.0 --port 9001
cargo run --manifest-path rust/shenyang_mahjong/Cargo.toml -- --host 0.0.0.0 --port 9002
cargo run --manifest-path rust/holdem/Cargo.toml -- --host 0.0.0.0 --port 9003
cargo run --manifest-path rust/tractor/Cargo.toml -- --host 0.0.0.0 --port 9004
P2P_TURN_SECRET='replace-with-a-long-random-secret' \
P2P_TURN_PUBLIC_IP='203.0.113.10' \
cargo run --manifest-path rust/p2p/Cargo.toml -- --host 0.0.0.0 --port 9005
```

`p2p` 会在同一 Rust 进程内监听 UDP 3478 提供 STUN/TURN，并使用 UDP
49160-49200 作为 relay 端口；不依赖外部 coturn。局域网运行可以省略
`P2P_TURN_PUBLIC_IP` 自动选择本机地址，公网 NAT 部署必须配置公网 IP 和端口映射。

参数：

- `--host 0.0.0.0`: 局域网可访问。
- `--host 127.0.0.1`: 只允许本机访问。
- `--port`: 指定端口。

## 检查和测试

```sh
cargo check --manifest-path rust/landlord/Cargo.toml
cargo test --manifest-path rust/landlord/Cargo.toml
cargo check --manifest-path rust/shenyang_mahjong/Cargo.toml
cargo check --manifest-path rust/holdem/Cargo.toml
cargo check --manifest-path rust/tractor/Cargo.toml
cargo check --manifest-path rust/p2p/Cargo.toml
cargo test --manifest-path rust/tractor/Cargo.toml
cargo test --manifest-path rust/p2p/Cargo.toml
```

拖拉机房间开始后会锁定设置。当前主要设置包括：`deck_count`（几副牌）、`removed_rank_count`（按 `3/4/6/7/8/9/J/Q/A` 的顺序删掉前 N 个点数，`0` 表示不删）、`first_deal_time`（首局发牌总时间，毫秒）、`deal_time`（后续局发牌总时间，毫秒）、`ai_action_time`（AI/托管行动间隔，毫秒）、`target_rank`（最终目标 rank）、`blood_enabled` / `blood_start_score` / `blood_score_per_unit`（喝血相关）。首局发牌中由所有玩家抢主/反主并决定首庄；第二局起只由既定庄家选择主花色。发完后庄家收底并扣回相同张数，随后进入出牌。

## 发布 Rust WS 服务端

公版自建 WS 服务端不包含 official 统计、SQLite 或游戏 AI，也不接受添加/删除 AI
座位。斗地主、沈阳麻将和拖拉机需要真人凑齐人数；超时只执行保证牌局可继续的合法兜底动作。
官方服的统计与 AI 源码保存在私有仓库，并由私有 `official` 构建接入。

推荐下载 Linux x86_64 musl release 产物。该产物是静态单文件，适合大多数 x86_64 Linux 服务器直接运行。

### Linux x86_64 musl

从源码构建 Linux x86_64 musl release：

```sh
sudo apt install -y musl-tools
rustup target add x86_64-unknown-linux-musl
```

构建发布包：

```sh
./build_script/build_all.sh
```

产物位置：

```sh
target/x86_64-unknown-linux-musl/release/landlord
target/x86_64-unknown-linux-musl/release/shenyang_mahjong
target/x86_64-unknown-linux-musl/release/holdem
target/x86_64-unknown-linux-musl/release/tractor
target/x86_64-unknown-linux-musl/release/p2p
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
./build_script/build_all.sh
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
  -p tractor `
  -p p2p
Remove-Item Env:RUSTFLAGS
```

产物位置：

```powershell
target\x86_64-pc-windows-msvc\release\landlord.exe
target\x86_64-pc-windows-msvc\release\shenyang_mahjong.exe
target\x86_64-pc-windows-msvc\release\holdem.exe
target\x86_64-pc-windows-msvc\release\tractor.exe
target\x86_64-pc-windows-msvc\release\p2p.exe
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
