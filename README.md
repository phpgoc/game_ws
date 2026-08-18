# game_ws

`ws` 是可开源的 WebSocket 游戏服务端目录。

[![Public WS CI](https://github.com/phpgoc/game_ws/actions/workflows/ci.yml/badge.svg)](https://github.com/phpgoc/game_ws/actions/workflows/ci.yml)

## License

本目录以 MIT License 发布；详见 [LICENSE](LICENSE)。

## 目录

- `share_type_public/`: 公共协议类型，用于 web / ws / Android。
- `rust/common/`: Rust WS 公共库，包含房间、连接、运行时。
- `rust/landlord/`: 斗地主 Rust 服务端。
- `rust/shenyang_mahjong/`: 沈阳麻将 Rust 服务端。
- `rust/holdem/`: Hold'em 系列 Rust 服务端，承载德州、明牌德州、短牌德州和奥马哈。
- `rust/tractor/`: 拖拉机 Rust 服务端。
- `rust/upgrade_common/`: 拖拉机与升级共享的牌、等级和分数进阶原语。
- `rust/upgrade/`: 升级 Rust 服务端。
- `rust/p2p/`: 独立的两人 WebRTC 信令服务与 STUN/TURN 临时凭证签发器，不依赖其他游戏 crate。
- `rust/dominoes/`: 3–4 人双六西洋骨牌 WS 服务，默认端口 9007；公开构建不包含 AI。
- `rust/android_server/`: Android JNI bridge，按单游戏 feature 产出 `libws_server.so`。
- `android/`: 7 个 Rust 服务共用的 Android 前台服务壳，每个 APK 只打包一个 `libws_server.so`。

## 支持平台与发布规则

当前 7 个 server 都支持从源码编译：

- `landlord`
- `shenyang_mahjong`
- `holdem`
- `tractor`
- `dominoes`
- `upgrade`
- `p2p`

平台规则如下：

| 目标平台 | 编译方式 | 公开 release |
| --- | --- | --- |
| Linux x86_64 musl | `build_script/build_all.sh` | 是，发布 7 个静态可执行文件 |
| Windows x86_64 | 按本文 PowerShell 命令自行编译 | 否 |
| Android APK | 使用 Gradle 或 `Dockerfile.android` 自行编译 | 否 |
| ARM64 Linux | 使用交叉工具链或 `Dockerfile.arm64` 自行编译 | 否 |

`build_script/build_all.sh` 和 `build_script/build_in_docker.sh` 只负责正式发布的
7 个 Linux x86_64 musl server，不再构建 APK、Windows 或 ARM Linux 产物。

独立检出开源仓库后，手动运行 Cargo 或 Gradle 命令前先准备不包含私有实现的依赖边界：

```sh
./ci/prepare-public-build.sh
```

Windows 用户可在 Git Bash 中执行 `bash ci/prepare-public-build.sh`。
在完整 `lan_game` 仓库中构建时，脚本会直接使用已有的真实 sibling crate。

## 运行 Rust WS 服务

本地端口与协议 `GameId` 的对应关系固定如下。端口只用于本机/局域网部署，
客户端 JOIN 仍必须发送表中的 `GameId`；不要因为端口连续就推算游戏编号。

| GameId | 游戏/服务 | 本地 WS 端口 |
| ---: | --- | ---: |
| 1 | 斗地主 (`landlord`) | 9001 |
| 2 | 沈阳麻将 (`shenyang_mahjong`) | 9002 |
| 3、5、6、7 | 德州扑克合集 (`holdem`) | 9003 |
| 4 | 拖拉机 (`tractor`) | 9004 |
| 9 | P2P 信令 (`p2p`) | 9005 |
| 10 | 升级 (`upgrade`) | 9006 |
| 11 | 西洋骨牌 (`dominoes`) | 9007 |

`GameId=8` 是随机匹配会员权益，不对应一个官方游戏 WS 端口；匹配服务使用
9010（谁是地主匹配 9011、合作德州匹配 9012）。

在公版仓库根目录运行：

```sh
cargo run --manifest-path rust/landlord/Cargo.toml -- --host 0.0.0.0 --port 9001
cargo run --manifest-path rust/shenyang_mahjong/Cargo.toml -- --host 0.0.0.0 --port 9002
cargo run --manifest-path rust/holdem/Cargo.toml -- --host 0.0.0.0 --port 9003
cargo run --manifest-path rust/tractor/Cargo.toml -- --host 0.0.0.0 --port 9004
P2P_TURN_SECRET='replace-with-a-long-random-secret' \
P2P_TURN_PUBLIC_IP='203.0.113.10' \
cargo run --manifest-path rust/p2p/Cargo.toml -- --host 0.0.0.0 --port 9005
cargo run --manifest-path rust/upgrade/Cargo.toml -- --host 0.0.0.0 --port 9006
cargo run --manifest-path rust/dominoes/Cargo.toml -- --host 0.0.0.0 --port 9007
```

`p2p` 会在同一 Rust 进程内监听 UDP 3478 提供 STUN/TURN，并使用 UDP
49160-49200 作为 relay 端口；不依赖外部 coturn。局域网运行可以省略
`P2P_TURN_PUBLIC_IP` 自动选择本机地址，公网 NAT 部署必须配置公网 IP 和端口映射。
客户端会先使用 STUN 尝试直连；双方都报告直连失败后，服务才自动签发短期 TURN
凭证并切换到 relay，避免可直连时无谓占用中继流量。

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
cargo check --manifest-path rust/upgrade_common/Cargo.toml
cargo check --manifest-path rust/upgrade/Cargo.toml
cargo check --manifest-path rust/p2p/Cargo.toml
cargo test --manifest-path rust/tractor/Cargo.toml
cargo test --manifest-path rust/upgrade_common/Cargo.toml
cargo test --manifest-path rust/upgrade/Cargo.toml
cargo test --manifest-path rust/p2p/Cargo.toml
```

公开仓库的 `Public WS CI` 在 push、pull request、手动触发和每周定时任务中免费运行：

- 对公开 Rust crate 和 Android bridge crate 执行 `rustfmt`、全部 target 测试和 `clippy -D warnings`；
- 对 7 个服务分别构建 Linux x86_64 musl 静态 release；
- 对 7 个服务分别构建同时包含 arm64-v8a 与 x86_64 的 Android APK，覆盖 JNI、NDK、Gradle 和 Kotlin 包装；
- 不启用依赖私有 `data` 的 `official` feature，不读取 secrets，不上传 artifact；Cargo 和 Gradle 依赖缓存按全部 `Cargo.toml` 的内容生成键。

独立检出开源仓库时，Cargo 和 rustfmt 仍需解析可选的私有 `data`、`runtime_common` 路径及仅供官方版使用的外部 AI 模块。
CI 仅把 `.github/fixtures` 中的空边界链接到预期位置，使未启用的依赖和条件模块能够完成解析；fixture
不包含私有实现，也不能用于构建 `official` feature。根仓库作为子模块使用时会继续解析真实的 `data`
crate、`runtime_common` 和 AI 模块。

`ws` 根目录不提交 `Cargo.lock`，构建、测试和元数据检查均根据当前 manifests 解析依赖，也不使用 `--locked`。需要在依赖已经缓存的环境中离线验证时，可使用 `--offline`；它只禁止联网，并不锁定依赖版本。嵌入主仓库时，Cargo 仍会按实际 workspace 和私有 sibling 依赖解析完整依赖图。

拖拉机和升级房间开始后都会锁定设置。拖拉机支持 2–3 副完整牌组，不提供删牌设置；升级支持 3–6 副，并可配置从低点数起删除牌面。两个游戏都不支持喝血、进贡或上贡，均按 `attacking_win_score`、`score_per_level` 和 `shutout_bonus_levels` 结算跳级。首局从 3（被删时取首个保留等级）开始，发牌中允许抢主/反主并决定首庄；后续局发牌时不亮主，庄家拿底后在同一个底牌操作窗口内先选主、再埋底。庄家方过庄后由庄家对家接庄，闲家上台后由原庄家下家接庄。首局默认总发牌 15 秒，后续局 3 秒；底牌操作窗口为出牌时间的 3 倍，选主不会重置倒计时。

## 编译与发布 Rust WS 服务端

公版自建 WS 服务端不包含 official 统计、SQLite 或游戏 AI，也不接受添加/删除 AI
座位。斗地主、沈阳麻将和拖拉机需要真人凑齐人数；超时只执行保证牌局可继续的合法兜底动作。
AI 源码保存在私有仓库；`official` feature 接入服务端 AI，并额外启用会员校验与统计数据。
JOIN 响应通过 `supports_ai_players` 声明当前 WS 实例是否支持 AI，客户端不得根据连接地址或自身打包类型推断该能力。

推荐下载 Linux x86_64 musl release 产物。该产物是静态单文件，适合大多数 x86_64 Linux 服务器直接运行。

### Linux x86_64 musl（公开 release）

Ubuntu / Debian 安装依赖：

```sh
sudo apt update
sudo apt install -y build-essential musl-tools pkg-config
rustup toolchain install stable
rustup target add x86_64-unknown-linux-musl
```

在 `ws` 根目录构建全部 7 个发布文件：

```sh
./build_script/build_all.sh
```

产物位置：

```text
build_script/output/landlord
build_script/output/shenyang_mahjong
build_script/output/holdem
build_script/output/tractor
build_script/output/upgrade
build_script/output/p2p
```

不想在主机安装 Rust 和 musl 工具链时，可用 Docker 构建相同的 7 个文件：

```sh
./build_script/build_in_docker.sh
```

#### macOS 交叉编译 Linux musl

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

也可用 `./build_script/install_deps_mac.sh` 安装这套 x86_64 Linux 发布工具链。

### Android APK（用户自行编译）

每个 APK 只包含一个 server。`-Pgame` 可填写 `landlord`、
`shenyang_mahjong`、`holdem`、`tractor`、`dominoes`、`upgrade` 或 `p2p`。

本机安装 JDK 17、Android SDK 35、NDK 27 后，再安装 Rust Android 工具链：

```sh
cargo install cargo-ndk --version 4.1.2 --locked
rustup target add aarch64-linux-android x86_64-linux-android
```

构建可直接安装的 debug APK；默认同时包含 ARM64 真机与 x86_64 模拟器：

```sh
cd android
./gradlew --no-daemon :app:assembleDebug -Pgame=tractor
```

产物位于 `android/app/build/outputs/apk/debug/app-debug.apk`。Windows 主机把
`./gradlew` 换成 `.\gradlew.bat`。完整的环境变量、单 ABI、release APK 与签名说明见
[`android/README.md`](android/README.md)。

也可以完全用 Docker 编译。下面以拖拉机 APK 为例，替换 `GAME` 即可编译其余 6 个：

```sh
mkdir -p build_script/output/android
docker build \
  --file build_script/Dockerfile.android \
  --build-arg GAME=tractor \
  --output type=local,dest=build_script/output/android \
  .
```

产物为 `build_script/output/android/tractor.apk`。该 Dockerfile 默认打包
`arm64-v8a,x86_64`；只要真机版本时增加 `--build-arg RUST_ABIS=arm64-v8a`。

### ARM64 Linux（用户自行编译）

推荐直接使用独立 Dockerfile 交叉编译 7 个 ARM64 GNU/Linux server：

```sh
mkdir -p build_script/output/arm64
docker build \
  --file build_script/Dockerfile.arm64 \
  --output type=local,dest=build_script/output/arm64 \
  .
```

输出目录包含 `landlord`、`shenyang_mahjong`、`holdem`、`tractor`、`dominoes`、`upgrade`、`p2p`。
这些是 `aarch64-unknown-linux-gnu` 文件，适用于 64 位 ARM Linux；它们会动态依赖
glibc。Dockerfile 使用 Ubuntu 20.04，以兼容 glibc 2.31 及以上系统。需要兼容更旧系统时，
应在对应旧版 Linux 镜像或目标 ARM 设备上重新编译。

不用 Docker 时，可以在 Ubuntu / Debian x86_64 主机安装 ARM64 交叉编译器：

```sh
sudo apt update
sudo apt install -y build-essential gcc-aarch64-linux-gnu pkg-config
rustup target add aarch64-unknown-linux-gnu
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc

cargo build --release \
  --target aarch64-unknown-linux-gnu \
  --manifest-path Cargo.toml \
  -p landlord \
  -p shenyang_mahjong \
  -p holdem \
  -p tractor \
    -p dominoes \
    -p upgrade \
  -p p2p \
  --no-default-features
```

本机构建产物位于 `target/aarch64-unknown-linux-gnu/release/`。如果设备显示
`armv7l` 而不是 `aarch64`，则需改用 `armv7-unknown-linux-gnueabihf` target 和
`gcc-arm-linux-gnueabihf` linker；ARMv7 不使用 `Dockerfile.arm64`。

### Windows x86_64（兼容构建，不进入 release）

Windows 不是推荐运行环境。WS 服务端即使能运行，也要额外考虑 Windows 防火墙、局域网发现、杀毒软件、端口开放和执行策略等问题。Windows release 仅用于验证或自行构建；公开发布页优先提供 Linux musl 静态产物。

Windows 版本应在 Windows 原生环境编译，不使用 Linux Docker 交叉编译 MSVC。
先安装 Rust stable，并在 Visual Studio Installer 中安装“使用 C++ 的桌面开发”和
Windows SDK。然后在 `ws` 根目录的 PowerShell 中静态链接 MSVC CRT：

```powershell
rustup target add x86_64-pc-windows-msvc
$env:RUSTFLAGS = "-C target-feature=+crt-static"
try {
  cargo build --release `
    --target x86_64-pc-windows-msvc `
    --manifest-path Cargo.toml `
    -p landlord `
    -p shenyang_mahjong `
    -p holdem `
    -p tractor `
    -p upgrade `
    -p p2p `
    --no-default-features
} finally {
  Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue
}
```

产物位置：

```powershell
target\x86_64-pc-windows-msvc\release\landlord.exe
target\x86_64-pc-windows-msvc\release\shenyang_mahjong.exe
target\x86_64-pc-windows-msvc\release\holdem.exe
target\x86_64-pc-windows-msvc\release\tractor.exe
target\x86_64-pc-windows-msvc\release\dominoes.exe
target\x86_64-pc-windows-msvc\release\upgrade.exe
target\x86_64-pc-windows-msvc\release\p2p.exe
```

例如运行拖拉机 server：

```powershell
.\target\x86_64-pc-windows-msvc\release\tractor.exe --host 0.0.0.0 --port 9004
```

首次运行要允许 Windows 防火墙放行对应 TCP 端口。`p2p` 还需要放行 UDP 3478 和
UDP 49160-49200；公网部署还要配置路由器端口映射。

### 维护约束

维护 release 脚本、CI 或自动生成的构建说明时，保持以下约束：

```text
推荐 release 产物：Linux x86_64 musl 静态单文件。
release 包范围：landlord、shenyang_mahjong、holdem、tractor、dominoes、upgrade、p2p。
build_all.sh 和 build_in_docker.sh 只生成上述 7 个 Linux x86_64 文件。
Android APK 与 ARM64 Linux 可以使用独立 Dockerfile，但只能由用户按文档自行构建，不进入 release。
Windows 不作为推荐运行环境；如需 Windows 构建说明，只保留 x86_64-pc-windows-msvc + crt-static 的验证命令，并提醒防火墙、杀毒软件、端口开放和执行策略需要额外处理。
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
