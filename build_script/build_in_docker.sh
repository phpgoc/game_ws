#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WS_DIR="$(dirname "${SCRIPT_DIR}")"
IMAGE="${WS_BUILDER_IMAGE:-lan-game-ws-builder}"
OUTPUT_DIR="${SCRIPT_DIR}/output"

command -v docker >/dev/null 2>&1 || {
    echo "Docker is required." >&2
    exit 1
}

mkdir -p "${OUTPUT_DIR}"

echo "=== [1/2] Building ${IMAGE} ==="
docker build \
    --platform linux/amd64 \
    --tag "${IMAGE}" \
    --file "${SCRIPT_DIR}/Dockerfile" \
    "${WS_DIR}"

echo
echo "=== [2/2] Building 10 artifacts in Docker ==="
docker run \
    --rm \
    --platform linux/amd64 \
    --volume "${OUTPUT_DIR}:/workspace/build_script/output" \
    "${IMAGE}"

echo
echo "=== Done ==="
ls -lh "${OUTPUT_DIR}"
