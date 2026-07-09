#!/usr/bin/env bash
# Install the official upstream Rust toolchain pinned by rust-toolchain.toml.
#
# `riscv32imfc-unknown-none-elf` is a rustc builtin target in this nightly, but
# rustup does not provide a prebuilt std component for it yet. CI therefore
# installs rust-src and RISC-V build commands pass `-Zbuild-std=core,alloc`.
set -euo pipefail

TOOLCHAIN="${RUST_TOOLCHAIN:-nightly-2026-07-09}"

rustup toolchain install "$TOOLCHAIN" \
  --profile minimal \
  --component rust-src \
  --component clippy \
  --component rustfmt \
  --component llvm-tools-preview

rustc +"$TOOLCHAIN" --version
rustc +"$TOOLCHAIN" --print target-list | grep -qx 'riscv32imfc-unknown-none-elf'

if rustup target list --toolchain "$TOOLCHAIN" | grep -Eq '^riscv32imfc-unknown-none-elf([[:space:]]|$)'; then
  echo "rustup ships prebuilt rust-std for riscv32imfc-unknown-none-elf"
else
  echo "rustup has no prebuilt rust-std for riscv32imfc-unknown-none-elf; using -Zbuild-std=core,alloc"
fi
