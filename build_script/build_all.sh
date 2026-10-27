#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WS_DIR="$(dirname "${SCRIPT_DIR}")"
OUTPUT_DIR="${OUTPUT_DIR:-${SCRIPT_DIR}/output}"
BUILD_DIR="${BUILD_DIR:-${SCRIPT_DIR}/target}"
LINUX_TARGET="x86_64-unknown-linux-musl"
SERVICES=(ws_server p2p)
SERVICE_COUNT="${#SERVICES[@]}"

"${WS_DIR}/ci/prepare-public-build.sh"

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

echo "=== Building ${SERVICE_COUNT} Linux x86_64 musl release executables ==="
for service in "${SERVICES[@]}"; do
    case "${service}" in
        ws_server) manifest="${WS_DIR}/rust/server/Cargo.toml" ;;
        p2p) manifest="${WS_DIR}/rust/p2p/Cargo.toml" ;;
        *) echo "Unknown service: ${service}" >&2; exit 2 ;;
    esac
    echo "--- ${service}"
    CARGO_TARGET_DIR="${BUILD_DIR}/linux" cargo build \
        --release \
        --target "${LINUX_TARGET}" \
        --manifest-path "${manifest}" \
        -p "${service}" \
        --bin "${service}" \
        --no-default-features
    install -m 0755 \
        "${BUILD_DIR}/linux/${LINUX_TARGET}/release/${service}" \
        "${OUTPUT_DIR}/${service}"
done

expected=()
for service in "${SERVICES[@]}"; do
    expected+=("${OUTPUT_DIR}/${service}")
done
for artifact in "${expected[@]}"; do
    [[ -s "${artifact}" ]] || {
        echo "Missing or empty artifact: ${artifact}" >&2
        exit 1
    }
done

artifact_count="$(find "${OUTPUT_DIR}" -maxdepth 1 -type f | wc -l | tr -d ' ')"
if [[ "${artifact_count}" != "${SERVICE_COUNT}" ]]; then
    echo "Expected exactly ${SERVICE_COUNT} artifacts, found ${artifact_count}" >&2
    exit 1
fi

echo
echo "=== Built ${SERVICE_COUNT} Linux x86_64 musl artifacts in ${OUTPUT_DIR} ==="
ls -lh "${OUTPUT_DIR}"
