#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WS_DIR="$(dirname "${SCRIPT_DIR}")"
OUTPUT_DIR="${OUTPUT_DIR:-${SCRIPT_DIR}/output}"
BUILD_DIR="${BUILD_DIR:-${SCRIPT_DIR}/target}"
JNI_DIR="${BUILD_DIR}/jniLibs"
ANDROID_JNI_DIR="${WS_DIR}/android/app/src/main/jniLibs"
LINUX_TARGET="x86_64-unknown-linux-musl"
ANDROID_PLATFORM="${ANDROID_PLATFORM:-26}"
GRADLE="${GRADLE:-${WS_DIR}/android/gradlew}"
GAMES=(landlord shenyang_mahjong holdem tractor p2p)
ANDROID_ABIS=(arm64-v8a x86_64)

if [[ "$(uname -s)" == "Darwin" ]]; then
    if ! command -v java >/dev/null 2>&1 && command -v brew >/dev/null 2>&1; then
        export JAVA_HOME="$(brew --prefix openjdk@17)/libexec/openjdk.jdk/Contents/Home"
        export PATH="${JAVA_HOME}/bin:${PATH}"
    fi
    export ANDROID_HOME="${ANDROID_HOME:-${HOME}/Library/Android/sdk}"
    if [[ -z "${ANDROID_NDK_HOME:-}" && -d "${ANDROID_HOME}/ndk/27.0.12077973" ]]; then
        export ANDROID_NDK_HOME="${ANDROID_HOME}/ndk/27.0.12077973"
    fi
fi

require_command() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "Missing required command: $1" >&2
        exit 1
    }
}

cleanup() {
    rm -rf "${ANDROID_JNI_DIR}"
}
trap cleanup EXIT

require_command cargo
require_command java
if ! cargo ndk --version >/dev/null 2>&1; then
    echo "cargo-ndk is required. Run: cargo install cargo-ndk" >&2
    exit 1
fi

if [[ "$(uname -s)" == "Darwin" ]] && command -v x86_64-linux-musl-gcc >/dev/null 2>&1; then
    export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=x86_64-linux-musl-gcc
fi

mkdir -p "${OUTPUT_DIR}" "${BUILD_DIR}" "${JNI_DIR}"
find "${OUTPUT_DIR}" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +
rm -rf "${JNI_DIR}"
mkdir -p "${JNI_DIR}"

echo "=== [1/3] Building 5 Linux x86_64 musl executables ==="
for game in "${GAMES[@]}"; do
    echo "--- ${game}"
    CARGO_TARGET_DIR="${BUILD_DIR}/linux" cargo build \
        --release \
        --target "${LINUX_TARGET}" \
        --manifest-path "${WS_DIR}/rust/${game}/Cargo.toml" \
        --bin "${game}"
    install -m 0755 \
        "${BUILD_DIR}/linux/${LINUX_TARGET}/release/${game}" \
        "${OUTPUT_DIR}/${game}"
done

echo
echo "=== [2/3] Building 5 Android native libraries ==="
for game in "${GAMES[@]}"; do
    echo "--- ${game}"
    (
        cd "${WS_DIR}/rust/${game}"
        CARGO_TARGET_DIR="${BUILD_DIR}/android" cargo ndk \
            -t "${ANDROID_ABIS[0]}" \
            -t "${ANDROID_ABIS[1]}" \
            --platform "${ANDROID_PLATFORM}" \
            -o "${JNI_DIR}/${game}" \
            build \
            --release \
            --lib
    )
done

echo
echo "=== [3/3] Packaging 5 Android APKs with the shared wrapper ==="
for game in "${GAMES[@]}"; do
    echo "--- ${game}"
    rm -rf "${ANDROID_JNI_DIR}" "${WS_DIR}/android/app/build"
    mkdir -p "${ANDROID_JNI_DIR}"
    cp -R "${JNI_DIR}/${game}/." "${ANDROID_JNI_DIR}/"

    (
        cd "${WS_DIR}/android"
        "${GRADLE}" --no-daemon --console=plain :app:assembleDebug \
            -Pgame="${game}" \
            -PrustAbis="$(IFS=,; echo "${ANDROID_ABIS[*]}")" \
            -PskipRustBuild=true
    )

    apk="${WS_DIR}/android/app/build/outputs/apk/debug/app-debug.apk"
    if [[ ! -f "${apk}" ]]; then
        echo "Gradle did not produce the expected APK: ${apk}" >&2
        exit 1
    fi
    install -m 0644 "${apk}" "${OUTPUT_DIR}/${game}.apk"
done

cleanup
trap - EXIT

expected=()
for game in "${GAMES[@]}"; do
    expected+=("${OUTPUT_DIR}/${game}" "${OUTPUT_DIR}/${game}.apk")
done
for artifact in "${expected[@]}"; do
    [[ -s "${artifact}" ]] || {
        echo "Missing or empty artifact: ${artifact}" >&2
        exit 1
    }
done

artifact_count="$(find "${OUTPUT_DIR}" -maxdepth 1 -type f | wc -l | tr -d ' ')"
if [[ "${artifact_count}" != "10" ]]; then
    echo "Expected exactly 10 artifacts, found ${artifact_count}" >&2
    exit 1
fi

echo
echo "=== Built 10 artifacts in ${OUTPUT_DIR} ==="
ls -lh "${OUTPUT_DIR}"
