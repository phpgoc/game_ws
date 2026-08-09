#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WS_DIR="$(dirname "${SCRIPT_DIR}")"
OUTPUT_DIR="${OUTPUT_DIR:-${SCRIPT_DIR}/output}"
BUILD_DIR="${BUILD_DIR:-${SCRIPT_DIR}/target}"
LINUX_TARGET="x86_64-unknown-linux-musl"
GAMES=(landlord shenyang_mahjong holdem tractor upgrade dominoes p2p)
GAME_COUNT="${#GAMES[@]}"

"${WS_DIR}/ci/prepare-public-build.sh"

CARGO_LOCK_ARGS=()
if [[ "${WS_CARGO_LOCKED:-0}" == "1" || -L "${WS_DIR}/../data" || -L "${WS_DIR}/../runtime_common" || -L "${WS_DIR}/../ai" ]]; then
    CARGO_LOCK_ARGS+=(--locked)
fi

require_command() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "Missing required command: $1" >&2
        exit 1
    }
}

require_command cargo

if [[ "$(uname -s)" == "Darwin" ]] && command -v x86_64-linux-musl-gcc >/dev/null 2>&1; then
    export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=x86_64-linux-musl-gcc
fi

mkdir -p "${OUTPUT_DIR}" "${BUILD_DIR}"
find "${OUTPUT_DIR}" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +

echo "=== Building ${GAME_COUNT} Linux x86_64 musl release executables ==="
for game in "${GAMES[@]}"; do
    echo "--- ${game}"
    CARGO_TARGET_DIR="${BUILD_DIR}/linux" cargo build \
        --release \
        --target "${LINUX_TARGET}" \
        --manifest-path "${WS_DIR}/Cargo.toml" \
        -p "${game}" \
        --bin "${game}" \
        --no-default-features \
        "${CARGO_LOCK_ARGS[@]}"
    install -m 0755 \
        "${BUILD_DIR}/linux/${LINUX_TARGET}/release/${game}" \
        "${OUTPUT_DIR}/${game}"
done

expected=()
for game in "${GAMES[@]}"; do
    expected+=("${OUTPUT_DIR}/${game}")
done
for artifact in "${expected[@]}"; do
    [[ -s "${artifact}" ]] || {
        echo "Missing or empty artifact: ${artifact}" >&2
        exit 1
    }
done

artifact_count="$(find "${OUTPUT_DIR}" -maxdepth 1 -type f | wc -l | tr -d ' ')"
if [[ "${artifact_count}" != "${GAME_COUNT}" ]]; then
    echo "Expected exactly ${GAME_COUNT} artifacts, found ${artifact_count}" >&2
    exit 1
fi

echo
echo "=== Built ${GAME_COUNT} Linux x86_64 musl artifacts in ${OUTPUT_DIR} ==="
ls -lh "${OUTPUT_DIR}"
