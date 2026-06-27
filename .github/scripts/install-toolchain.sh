#!/usr/bin/env bash
# Install the custom `hisi-riscv` toolchain (stable rustc with the riscv32imfc
# hard-float target baked in as a builtin) by extracting it straight into rustup's
# toolchains dir — rustup auto-discovers any directory there, so no
# `rustup toolchain link` is needed and the toolchain is self-contained.
# Idempotent. Used by the CI workflows since hisi-riscv-rs pins
# `channel = "hisi-riscv"` in rust-toolchain.toml. (CI runs on x86_64 Linux; the
# release also ships aarch64-linux, macOS x86_64/aarch64, and windows x86_64 tarballs.)
set -euo pipefail

VER="v1.96.0-2"
BASE="hisi-riscv-rust-1.96.0-x86_64-unknown-linux-gnu"
URL="https://github.com/hispark-rs/hisi-riscv-rust-toolchain/releases/download/${VER}/${BASE}.tar.gz"

if rustup toolchain list 2>/dev/null | grep -q '^hisi-riscv'; then
  echo "hisi-riscv toolchain already installed"
  exit 0
fi

TC_DIR="${RUSTUP_HOME:-$HOME/.rustup}/toolchains/hisi-riscv"
TMP="${RUNNER_TEMP:-/tmp}/hisi-riscv-toolchain.tar.gz"
echo "Downloading $URL"
curl -fsSL --retry 3 -o "$TMP" "$URL"
mkdir -p "$TC_DIR"
# The tarball's top-level dir is stage2/; --strip-components=1 drops it so that
# bin/lib/libexec land directly under <toolchains>/hisi-riscv/.
tar --strip-components=1 -C "$TC_DIR" -xzf "$TMP"
echo "installed hisi-riscv -> $TC_DIR"
rustc +hisi-riscv --version
