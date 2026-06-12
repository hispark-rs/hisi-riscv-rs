#!/usr/bin/env bash
# Install + rustup-link the custom `hisi-riscv` toolchain (stable rustc with the
# riscv32imfc hard-float target baked in as a builtin). Idempotent.
# Used by the CI workflows since hisi-riscv-rs pins `channel = "hisi-riscv"` in
# rust-toolchain.toml. (CI runs on x86_64 Linux; the release also ships
# aarch64-linux, macOS x86_64/aarch64, and windows x86_64 tarballs.)
set -euo pipefail

VER="v1.96.0-2"
BASE="hisi-riscv-rust-1.96.0-x86_64-unknown-linux-gnu"
URL="https://github.com/hispark-rs/hisi-riscv-rust-toolchain/releases/download/${VER}/${BASE}.tar.gz"

if rustup toolchain list 2>/dev/null | grep -q '^hisi-riscv'; then
  echo "hisi-riscv toolchain already linked"
  exit 0
fi

DEST="${RUNNER_TEMP:-/tmp}/hisi-riscv-toolchain"
mkdir -p "$DEST"
echo "Downloading $URL"
curl -fsSL --retry 3 -o "$DEST/tc.tar.gz" "$URL"
tar -C "$DEST" -xzf "$DEST/tc.tar.gz"
rustup toolchain link hisi-riscv "$DEST/stage2"
echo "linked hisi-riscv -> $DEST/stage2"
rustc +hisi-riscv --version
