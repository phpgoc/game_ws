#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WS_DIR="$(dirname "$SCRIPT_DIR")"

IMAGE="ws-builder"
OUTPUT_DIR="${WS_DIR}/build_script/output"
mkdir -p "${OUTPUT_DIR}"

echo "=== Step 1/2: docker build ${IMAGE} ==="
docker build \
    --platform linux/amd64 \
    -t "${IMAGE}" \
    -f "${SCRIPT_DIR}/Dockerfile" \
    "${WS_DIR}"

echo ""
echo "=== Step 2/2: docker run (build 10 artifacts) ==="
docker run \
    --rm \
    --platform linux/amd64 \
    -v "${OUTPUT_DIR}:/workspace/build_script/output" \
    "${IMAGE}" \
    /workspace/build_all.sh

echo ""
echo "=== Done ==="
ls -lh "${OUTPUT_DIR}/"
