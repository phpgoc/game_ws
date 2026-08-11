#!/usr/bin/env bash
set -euo pipefail

repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

install_rust_boundary() {
  local name="$1"
  local dependency="$2"
  local stub="$3"

  if [[ -e "$dependency" || -L "$dependency" ]]; then
    if [[ ! -f "$dependency/Cargo.toml" ]]; then
      echo "The existing sibling $name path is not a Rust crate: $dependency" >&2
      exit 2
    fi
    echo "Using the existing sibling $name crate; public features remain disabled."
  else
    ln -s "$stub" "$dependency"
    echo "Installed the empty public-CI $name boundary at $dependency."
  fi
}

install_rust_boundary \
  data \
  "$repository_root/../data" \
  "$repository_root/.github/fixtures/data"
install_rust_boundary \
  runtime_common \
  "$repository_root/../runtime_common" \
  "$repository_root/.github/fixtures/runtime_common"

ai_dependency="$repository_root/../ai"
ai_stub="$repository_root/.github/fixtures/ai"

if [[ -e "$ai_dependency" || -L "$ai_dependency" ]]; then
  for game in landlord shenyang_mahjong tractor upgrade dominoes; do
    if [[ ! -f "$ai_dependency/$game/src/embedded/mod.rs" ]]; then
      echo "The existing sibling AI path is missing $game/src/embedded/mod.rs: $ai_dependency" >&2
      exit 2
    fi
  done
  echo "Using the existing sibling AI modules; official features remain disabled."
else
  ln -s "$ai_stub" "$ai_dependency"
  echo "Installed the empty public-CI AI boundary at $ai_dependency."
fi
