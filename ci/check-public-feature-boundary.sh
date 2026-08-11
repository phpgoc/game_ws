#!/usr/bin/env bash
set -euo pipefail

repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
workflow="$repository_root/.github/workflows/ci.yml"

if rg -n -- '--all-features|features:.*official|--features.*official' "$workflow"; then
  echo "Public WS CI must never enable the private official feature." >&2
  exit 1
fi

for required_flag in \
  'cargo test .{0,300}--no-default-features' \
  'cargo clippy .{0,300}--no-default-features' \
  'cargo build .{0,300}--no-default-features'; do
  if ! rg -U -q "(?s)$required_flag" "$workflow"; then
    echo "Public WS CI is missing an explicit feature boundary: $required_flag" >&2
    exit 1
  fi
done

echo "Public WS CI explicitly excludes the official feature."
