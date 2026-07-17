#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "This installer is only for macOS." >&2
    exit 1
fi

command -v brew >/dev/null 2>&1 || {
    echo "Homebrew not found. Installing it..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
}

echo "=== [1/5] Installing JDK 17 and Linux musl cross compiler ==="
brew install openjdk@17
brew tap FiloSottile/musl-cross
brew install musl-cross

echo "=== [2/5] Installing Rust ==="
if ! command -v rustup >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1090
    source "${HOME}/.cargo/env"
fi

echo "=== [3/5] Installing Rust targets and cargo-ndk ==="
rustup target add \
    x86_64-unknown-linux-musl \
    aarch64-linux-android \
    x86_64-linux-android
cargo install cargo-ndk --locked

echo "=== [4/5] Installing Android command-line tools ==="
ANDROID_HOME="${ANDROID_HOME:-${HOME}/Library/Android/sdk}"
TOOLS_URL="https://dl.google.com/android/repository/commandlinetools-mac-11076708_latest.zip"
if [[ ! -x "${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager" ]]; then
    work_dir="$(mktemp -d)"
    trap 'rm -rf "${work_dir}"' EXIT
    mkdir -p "${ANDROID_HOME}/cmdline-tools"
    curl -fsSL -o "${work_dir}/command-line-tools.zip" "${TOOLS_URL}"
    unzip -q "${work_dir}/command-line-tools.zip" -d "${work_dir}"
    rm -rf "${ANDROID_HOME}/cmdline-tools/latest"
    mv "${work_dir}/cmdline-tools" "${ANDROID_HOME}/cmdline-tools/latest"
    rm -rf "${work_dir}"
    trap - EXIT
fi

echo "=== [5/5] Installing Android SDK 35 and NDK 27 ==="
yes | "${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager" \
    --sdk_root="${ANDROID_HOME}" \
    --licenses >/dev/null || true
"${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager" \
    --sdk_root="${ANDROID_HOME}" \
    "platform-tools" \
    "platforms;android-35" \
    "build-tools;34.0.0" \
    "build-tools;35.0.0" \
    "ndk;27.0.12077973"

cat <<EOF

Dependencies installed. Add these lines to your shell profile:

export JAVA_HOME="$(brew --prefix openjdk@17)/libexec/openjdk.jdk/Contents/Home"
export ANDROID_HOME="${ANDROID_HOME}"
export ANDROID_NDK_HOME="${ANDROID_HOME}/ndk/27.0.12077973"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="x86_64-linux-musl-gcc"
export PATH="\${JAVA_HOME}/bin:\${ANDROID_HOME}/cmdline-tools/latest/bin:\${ANDROID_HOME}/platform-tools:\${PATH}"

Then run: ./build_script/build_all.sh
EOF
