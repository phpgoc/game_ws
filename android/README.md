# Android WS Server Wrapper

这个目录是一套供 5 个服务共用的 Android 前台服务壳：

- `landlord`（斗地主，端口 9001）
- `shenyang_mahjong`（沈阳麻将，端口 9002）
- `holdem`（德州扑克合集，端口 9003）
- `tractor`（拖拉机，端口 9004）
- `p2p`（P2P 信令与内置 STUN/TURN，端口 9005）

Kotlin 负责 Activity、前台 Service、通知、WakeLock/WifiLock 和状态展示；
WebSocket、房间及游戏逻辑由对应的 Rust `cdylib` 提供。每个 APK 只包含一个游戏的
`.so`，通过相同的 JNI 接口调用，因此不需要复制 Android 工程。

## 依赖

- JDK 17
- Android SDK Platform / Build Tools 35
- Android NDK 27
- `cargo-ndk`
- Rust targets `aarch64-linux-android`、`x86_64-linux-android`

macOS 可在 `ws` 目录运行：

```sh
./build_script/install_deps_mac.sh
```

## 构建一个 APK

用 `-Pgame` 选择服务；省略时默认构建 `landlord`：

```sh
cd android
./gradlew --no-daemon :app:assembleDebug -Pgame=tractor
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

## 构建全部发布产物

不要逐个调用 Gradle，直接从 `ws` 目录运行统一脚本：

```sh
./build_script/build_all.sh
```

或让 Docker 安装并隔离全部 Linux / Android 工具链：

```sh
./build_script/build_in_docker.sh
```

两者都会在 `build_script/output/` 生成 5 个 Linux musl 可执行文件和 5 个 APK。
