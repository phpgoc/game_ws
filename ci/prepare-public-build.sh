#!/usr/bin/env bash
set -euo pipefail

repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
data_dependency="$repository_root/../data"
data_stub="$repository_root/.github/fixtures/data"
ai_dependency="$repository_root/../ai"
ai_stub="$repository_root/.github/fixtures/ai"

if [[ -e "$data_dependency" || -L "$data_dependency" ]]; then
  if [[ ! -f "$data_dependency/Cargo.toml" ]]; then
    echo "The existing sibling data path is not a Rust crate: $data_dependency" >&2
    exit 2
  fi
  echo "Using the existing sibling data crate; public features remain disabled."
else
  ln -s "$data_stub" "$data_dependency"
  echo "Installed the empty public-CI data boundary at $data_dependency."
fi

if [[ -e "$ai_dependency" || -L "$ai_dependency" ]]; then
  for game in landlord shenyang_mahjong tractor; do
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
