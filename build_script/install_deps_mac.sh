#!/usr/bin/env bash
set -euo pipefail

echo "=== Installing build dependencies on macOS ==="

command -v brew >/dev/null 2>&1 || {
    echo "Homebrew not found. Installing..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
}

echo "[1/4] Installing JDK 17..."
brew install openjdk@17 2>/dev/null || echo "  JDK 17 already installed (or check openjdk@17)"

echo "[2/4] Installing Rust..."
command -v rustup >/dev/null 2>&1 || {
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "${HOME}/.cargo/env"
}

echo "[3/4] Adding Rust targets..."
rustup target add x86_64-unknown-linux-musl
rustup target add aarch64-linux-android
rustup target add x86_64-linux-android

echo "[4/4] Installing cargo-ndk + Android SDK/NDK..."
cargo install cargo-ndk

ANDROID_HOME="${ANDROID_HOME:-${HOME}/Library/Android/sdk}"
ANDROID_CMDLINE_TOOLS_URL="https://dl.google.com/android/repository/commandlinetools-mac-11076708_latest.zip"

if [ ! -f "${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager" ]; then
    mkdir -p "${ANDROID_HOME}/cmdline-tools"
    curl -sSfLo /tmp/cmdline-tools.zip "${ANDROID_CMDLINE_TOOLS_URL}"
    unzip -qo /tmp/cmdline-tools.zip -d /tmp/cmdline-tools-tmp
    mv /tmp/cmdline-tools-tmp/cmdline-tools "${ANDROID_HOME}/cmdline-tools/latest"
    rm -rf /tmp/cmdline-tools.zip /tmp/cmdline-tools-tmp
fi

export ANDROID_HOME
yes | "${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager" --sdk_root="${ANDROID_HOME}" \
    "platform-tools" \
    "platforms;android-35" \
    "build-tools;35.0.0" \
    "ndk;27.0.12077973"

echo ""
echo "Add these to your ~/.zshrc or ~/.bashrc:"
echo "  export ANDROID_HOME=${ANDROID_HOME}"
echo "  export ANDROID_NDK_HOME=${ANDROID_HOME}/ndk/27.0.12077973"
echo "  export PATH=\"\${ANDROID_HOME}/cmdline-tools/latest/bin:\${ANDROID_HOME}/platform-tools:\${PATH}\""
echo ""
echo "=== Done ==="
