# Landlord Android WS Server

这个目录是 Android 版斗地主 WebSocket 服务。

现在 Android 端不是纯 Kotlin 实现。Kotlin 只负责：

- Activity 界面
- 前台 Service
- 通知栏
- WakeLock / WifiLock
- 显示连接数和房间数

真正的 WebSocket 服务、房间逻辑、斗地主规则都在：

```text
ws/rust/landlord
```

Gradle 构建 Android App 时，会先用 `cargo-ndk` 把 Rust 编译成：

```text
app/src/main/jniLibs/<abi>/liblandlord.so
```

然后 Kotlin 通过 JNI 调用它。

## 需要安装什么

### Android

安装 Android Studio，并在 SDK Manager 里安装：

- Android SDK Platform 35
- Android NDK
- Android SDK Build-Tools

### JDK

推荐使用 Android Studio 自带 JDK 17。

### Rust / NDK 工具

```bash
cargo install cargo-ndk
rustup target add aarch64-linux-android x86_64-linux-android
```

如果 `cargo-ndk` 找不到 NDK，设置环境变量：

```bash
export ANDROID_HOME="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/<你的ndk版本>"
```

例如本机可能是：

```bash
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/30.0.14904198"
```

## 命令行构建

进入目录：

```bash
cd ws/android
```

### 模拟器

大多数模拟器是 `x86_64`：

```bash
./gradlew --no-daemon :app:assembleDebug -PrustAbis=x86_64
```

### 真机

大多数 Android 真机是 `arm64-v8a`：

```bash
./gradlew --no-daemon :app:assembleDebug -PrustAbis=arm64-v8a
```

### 同时构建真机和模拟器

```bash
./gradlew --no-daemon :app:assembleDebug
```

生成 APK：

```text
app/build/outputs/apk/debug/app-debug.apk
```

## Android Studio 运行

1. Android Studio 打开 `ws/android`。
2. 等 Gradle Sync 完成。
3. 选择模拟器或真机。
4. 点击 Run。

第一次构建会比较慢，因为会先编译 Rust native library。

如果只想跑模拟器，可以在 Android Studio 的 Gradle 参数里加：

```text
-PrustAbis=x86_64
```

如果只想跑真机：

```text
-PrustAbis=arm64-v8a
```

## 只检查 Kotlin

如果你只是改 UI 或 Service，不想编 Rust：

```bash
./gradlew --no-daemon :app:compileDebugKotlin -x buildRustLandlord
```

## 常见问题

### cargo-ndk is required

执行：

```bash
cargo install cargo-ndk
```

### target not installed

模拟器：

```bash
rustup target add x86_64-linux-android
```

真机：

```bash
rustup target add aarch64-linux-android
```

### 找不到 NDK

先确认 SDK 目录：

```bash
ls "$ANDROID_HOME/ndk"
```

然后设置：

```bash
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/<版本号>"
```

### 模拟器启动时报找不到 liblandlord.so

通常是 ABI 不匹配。重新构建模拟器 ABI：

```bash
./gradlew --no-daemon :app:assembleDebug -PrustAbis=x86_64
```

### 真机启动时报找不到 liblandlord.so

重新构建真机 ABI：

```bash
./gradlew --no-daemon :app:assembleDebug -PrustAbis=arm64-v8a
```

## 服务地址

App 启动后会在界面显示局域网地址，例如：

```text
ws://192.168.1.20:9001
```

同一局域网里的 web 前端可以用这个地址连接。
