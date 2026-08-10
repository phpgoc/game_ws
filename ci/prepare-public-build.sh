#!/usr/bin/env bash
set -euo pipefail

# The public WS workspace is self-contained.  Official membership, statistics,
# logging and AI integrations are compiled only by private wrapper crates in
# the main repository, so a public checkout needs no private dependency stubs.

repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
ai_dependency="$repository_root/../ai"
ai_stub="$repository_root/.github/fixtures/ai"

for game in landlord shenyang_mahjong tractor upgrade dominoes; do
  if [[ ! -f "$ai_stub/$game/src/embedded/mod.rs" ]]; then
    echo "missing rustfmt-only AI path boundary: $ai_stub/$game/src/embedded/mod.rs" >&2
    exit 2
  fi
done

if [[ -e "$ai_dependency" || -L "$ai_dependency" ]]; then
  [[ -d "$ai_dependency" ]] || {
    echo "the sibling ai path is not a directory: $ai_dependency" >&2
    exit 2
  }
  for game in landlord shenyang_mahjong tractor upgrade dominoes; do
    if [[ ! -f "$ai_dependency/$game/src/embedded/mod.rs" ]]; then
      echo "missing AI path boundary: $ai_dependency/$game/src/embedded/mod.rs" >&2
      exit 2
    fi
  done
else
  ln -s "$ai_stub" "$ai_dependency"
fi

if rg -n --glob 'rust/*/Cargo.toml' '^official[[:space:]]*=' .; then
  echo "public WS manifests must not expose the official feature" >&2
  exit 1
fi

if rg -n --glob 'rust/*/Cargo.toml' 'path = "\.\./\.\./\.\./(data|runtime_common)"' .; then
  echo "public WS manifests must not depend on private sibling crates" >&2
  exit 1
fi

echo "Public WS build boundary is self-contained; official integrations are unavailable."
