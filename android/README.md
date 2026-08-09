# Android WS Server Wrapper

这个目录是一套供 7 个服务共用的 Android 前台服务壳：

- `landlord`（斗地主，端口 9001）
- `shenyang_mahjong`（沈阳麻将，端口 9002）
- `holdem`（德州扑克合集，端口 9003）
- `tractor`（拖拉机，端口 9004）
- `p2p`（P2P 信令与内置 STUN/TURN，端口 9005）
- `upgrade`（升级，端口 9006）
- `dominoes`（西洋骨牌，端口 9007）

Kotlin 负责 Activity、前台 Service、通知、WakeLock/WifiLock 和状态展示；
WebSocket、房间及游戏逻辑由对应的 Rust `cdylib` 提供。每个 APK 只包含一个游戏的
`libws_server.so`，通过相同的 JNI 接口调用，因此不需要复制 Android 工程。

## 依赖

- JDK 17
- Android SDK Platform 35 / Build Tools 35.0.0
- Android NDK `27.0.12077973`
- `cargo-ndk`
- Rust targets `aarch64-linux-android`、`x86_64-linux-android`

在 Android Studio 的 SDK Manager 中安装 SDK、Build Tools 和 NDK 后，配置环境变量。
Linux / macOS 示例：

```sh
export ANDROID_HOME="$HOME/Android/Sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
```

macOS 的 SDK 默认路径通常是 `$HOME/Library/Android/sdk`。Windows PowerShell 示例：

```powershell
$env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
$env:ANDROID_NDK_HOME = "$env:ANDROID_HOME\ndk\27.0.12077973"
```

安装 Rust 目标和 `cargo-ndk`：

```sh
rustup toolchain install stable
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk --version 4.1.2 --locked
```

如果这是独立检出的 `game_ws` 仓库，先在 `ws` 根目录运行：

```sh
./ci/prepare-public-build.sh
```

Windows 用户可在 Git Bash 中执行 `bash ci/prepare-public-build.sh`。完整 `lan_game`
仓库已经有真实 sibling crate，不需要创建 fixture。

## 选择 server

用 `-Pgame` 选择 APK 中唯一包含的 server；省略时默认是 `landlord`：

| `-Pgame` | 服务 | 默认端口 |
| --- | --- | --- |
| `landlord` | 斗地主 | 9001 |
| `shenyang_mahjong` | 沈阳麻将 | 9002 |
| `holdem` | 德州扑克合集 | 9003 |
| `tractor` | 拖拉机 | 9004 |
| `p2p` | P2P 信令与内置 STUN/TURN | 9005 |
| `upgrade` | 升级 | 9006 |
| `dominoes` | 西洋骨牌 | 9007 |

## 在本机编译 APK

Linux / macOS：

```sh
cd android
./gradlew --no-daemon :app:assembleDebug -Pgame=tractor
```

Windows PowerShell：

```powershell
cd android
.\gradlew.bat --no-daemon :app:assembleDebug -Pgame=tractor
```

默认同时包含真机 `arm64-v8a` 和模拟器 `x86_64`。也可只构建一个 ABI：

```sh
./gradlew --no-daemon :app:assembleDebug \
  -Pgame=shenyang_mahjong \
  -PrustAbis=arm64-v8a
```

Gradle 会先用 `cargo-ndk` 构建所选 Rust 库。产物为：

```text
app/build/outputs/apk/debug/app-debug.apk
```

该文件已用本机 debug key 签名，可以直接安装测试。连续编译不同 server 时，Gradle 会
覆盖同一个 `app-debug.apk`，因此要在每次构建后改名保存。例如一次编译全部 7 个：

```sh
mkdir -p ../build_script/output/android
for game in landlord shenyang_mahjong holdem tractor dominoes upgrade p2p; do
  ./gradlew --no-daemon :app:assembleDebug -Pgame="$game"
  cp app/build/outputs/apk/debug/app-debug.apk \
    "../build_script/output/android/${game}.apk"
done
```

需要 release APK 时运行：

```sh
./gradlew --no-daemon :app:assembleRelease -Pgame=tractor
```

未配置签名时，产物 `app/build/outputs/apk/release/app-release-unsigned.apk` 不能直接安装。
请使用自己的 keystore，通过 Android Studio 或 Android SDK 的 `apksigner` 签名；仓库不会
保存用户签名密钥。

## 使用 Docker 编译 APK

不在主机安装 JDK、Android SDK、NDK 和 Rust Android target 时，在 `ws` 根目录运行：

```sh
mkdir -p build_script/output/android
docker build \
  --file build_script/Dockerfile.android \
  --build-arg GAME=tractor \
  --output type=local,dest=build_script/output/android \
  .
```

产物为 `build_script/output/android/tractor.apk`。`GAME` 支持上表中的 7 个值。
默认同时构建两个 ABI；只打包 ARM64 真机版本：

```sh
docker build \
  --file build_script/Dockerfile.android \
  --build-arg GAME=tractor \
  --build-arg RUST_ABIS=arm64-v8a \
  --output type=local,dest=build_script/output/android \
  .
```

Docker 会缓存 JDK、SDK、NDK 和 Rust 工具链。编译全部 7 个 APK 时，依次替换 `GAME`
并重复命令即可，已有工具链层不会重复下载。

## 常见问题

- `cargo ndk` 找不到：确认 `cargo install cargo-ndk --version 4.1.2 --locked` 成功，并把 `$HOME/.cargo/bin` 加入 `PATH`。
- 找不到 SDK 或 NDK：检查 `ANDROID_HOME` 和 `ANDROID_NDK_HOME`，NDK 版本必须与实际目录一致。
- APK 无法安装：debug APK 可直接测试；release APK 必须先签名。
- 模拟器或真机报 ABI 不匹配：模拟器使用 `x86_64`，常见真机使用 `arm64-v8a`；不确定时保留默认双 ABI。
- Android 最低版本为 API 26，即 Android 8.0。

## 发布规则

Android APK 仅供用户自行构建，不进入官方 release。`build_script/build_all.sh` 和
`build_script/build_in_docker.sh` 只生成 7 个 Linux x86_64 musl server；
`Dockerfile.android` 只在用户执行本文命令时构建所选 APK。
