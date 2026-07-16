#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WS_DIR="$(dirname "$SCRIPT_DIR")"

OUTPUT_DIR="${WS_DIR}/build_script/output"
mkdir -p "${OUTPUT_DIR}"

LINUX_TARGET="x86_64-unknown-linux-musl"

GAMES=("landlord" "shenyang_mahjong" "holdem" "tractor" "p2p")

# ============================================================
# 1. 5 Linux (musl) executables
# ============================================================
echo "=== [1/3] Building 5 Linux (musl) executables ==="
for game in "${GAMES[@]}"; do
    echo "--- ${game} ---"
    cargo build \
        --release \
        --target "${LINUX_TARGET}" \
        --manifest-path "${WS_DIR}/rust/${game}/Cargo.toml"

    cp "${WS_DIR}/rust/${game}/target/${LINUX_TARGET}/release/${game}" \
       "${OUTPUT_DIR}/${game}"
done

# ============================================================
# 2. 5 Android .so libraries (arm64-v8a + x86_64)
# ============================================================
echo ""
echo "=== [2/3] Building 5 Android .so libraries ==="
JNILIBS_DIR="${WS_DIR}/build_script/jniLibs"
rm -rf "${JNILIBS_DIR}"

for game in "${GAMES[@]}"; do
    echo "--- ${game} ---"
    cargo ndk \
        -t arm64-v8a \
        -t x86_64 \
        --platform 26 \
        -o "${JNILIBS_DIR}/${game}" \
        build \
        --release \
        --manifest-path "${WS_DIR}/rust/${game}/Cargo.toml"
done

# ============================================================
# 3. 5 Android APKs
# ============================================================
echo ""
echo "=== [3/3] Building Android APKs ==="

# landlord — 有完整的 Android 项目，直接编
echo "--- landlord ---"
rm -rf "${WS_DIR}/android/app/src/main/jniLibs"
mkdir -p "${WS_DIR}/android/app/src/main/jniLibs"
cp -r "${JNILIBS_DIR}/landlord/"* "${WS_DIR}/android/app/src/main/jniLibs/"

(
    cd "${WS_DIR}/android"
    ./gradlew assembleDebug --no-daemon -q 2>&1 | tail -3
)

LANDLORD_APK=$(find "${WS_DIR}/android/app/build/outputs/apk/debug" -name "*.apk" 2>/dev/null | head -1)
if [ -n "${LANDLORD_APK}" ] && [ -f "${LANDLORD_APK}" ]; then
    cp "${LANDLORD_APK}" "${OUTPUT_DIR}/landlord.apk"
    echo "  -> ${OUTPUT_DIR}/landlord.apk"
fi

# 其余 4 个游戏 — 仅有 .so，APK 需要各自的 Android 项目后用同样方式打
for game in "shenyang_mahjong" "holdem" "tractor" "p2p"; do
    echo "--- ${game} (APK skipped — needs Android wrapper project)"
done

rm -rf "${JNILIBS_DIR}"

# ============================================================
echo ""
echo "=== Done ==="
echo "Output: ${OUTPUT_DIR}/"
ls -lh "${OUTPUT_DIR}/"
