#!/usr/bin/env bash
# Install + rustup-link the custom `ws63` toolchain (stable rustc with the WS63
# riscv32imfc hard-float target baked in as a builtin). Idempotent.
# Used by the CI workflows since ws63-rs pins `channel = "ws63"` in rust-toolchain.toml.
set -euo pipefail

VER="v1.96.0-1"
BASE="ws63-rust-1.96.0-x86_64-unknown-linux-gnu"
URL="https://github.com/hispark-rs/hisi-riscv-rust-toolchain/releases/download/${VER}/${BASE}.tar.gz"

if rustup toolchain list 2>/dev/null | grep -q '^ws63'; then
  echo "ws63 toolchain already linked"
  exit 0
fi

DEST="${RUNNER_TEMP:-/tmp}/ws63-toolchain"
mkdir -p "$DEST"
echo "Downloading $URL"
curl -fsSL --retry 3 -o "$DEST/tc.tar.gz" "$URL"
tar -C "$DEST" -xzf "$DEST/tc.tar.gz"
rustup toolchain link ws63 "$DEST/stage2"
echo "linked ws63 -> $DEST/stage2"
rustc +ws63 --version
