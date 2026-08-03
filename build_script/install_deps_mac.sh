#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "This installer is only for macOS." >&2
    exit 1
fi

command -v brew >/dev/null 2>&1 || {
    echo "Homebrew not found. Installing it..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
}

echo "=== [1/3] Installing Linux musl cross compiler ==="
brew tap FiloSottile/musl-cross
brew install musl-cross

echo "=== [2/3] Installing Rust ==="
if ! command -v rustup >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1090
    source "${HOME}/.cargo/env"
fi

echo "=== [3/3] Installing Linux musl Rust target ==="
rustup target add x86_64-unknown-linux-musl

cat <<EOF

Dependencies installed. Add these lines to your shell profile:

export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="x86_64-linux-musl-gcc"

Then run: ./build_script/build_all.sh
EOF
