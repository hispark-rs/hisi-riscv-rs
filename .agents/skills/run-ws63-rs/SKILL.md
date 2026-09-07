---
name: run-ws63-rs
description: Build, check, lint, and test the ws63-rs embedded HAL for HiSilicon WS63 (RISC-V). Use when asked to build, verify, run checks, or test hisi-hal, ws63-pac, or any crate in this workspace.
---

Paths below are relative to the parent repository root. This workspace contains
chip-neutral crates, WS63 integration crates, independently versioned submodules,
and target-specific examples. Discover the current member set from Cargo metadata;
do not rely on a fixed crate or file count.

## Toolchain (required)

ws63-rs builds with the official upstream Rust nightly pinned in
`rust-toolchain.toml`. The WS63 target is `riscv32imfc-unknown-none-elf`
(RV32IMFC, hardware single-float `ilp32f`, no atomics). rustc knows this target,
but rustup does not ship a prebuilt std component for it yet, so RISC-V builds use
`-Zbuild-std=core,alloc`. The default target is set in `.cargo/config.toml`.

Install the pinned toolchain first:

```bash
rustup toolchain install nightly-2026-07-09 \
  --profile minimal \
  --component rust-src \
  --component clippy \
  --component rustfmt \
  --component llvm-tools-preview
```

The `hisi-riscv-rust-toolchain` repo is now the upstream nightly radar, not the
default custom rustc install path.

## Build (agent path)

```bash
bash .agents/skills/run-ws63-rs/driver.sh all      # checks + host tests + docs + fmt + clippy
bash .agents/skills/run-ws63-rs/driver.sh check    # STA/AP checks + blinky release build
bash .agents/skills/run-ws63-rs/driver.sh test     # current host-test lanes
bash .agents/skills/run-ws63-rs/driver.sh doc      # WS63 rustdoc
bash .agents/skills/run-ws63-rs/driver.sh fmt      # cargo fmt --check
bash .agents/skills/run-ws63-rs/driver.sh clippy   # cargo clippy
```

All steps target `riscv32imfc-unknown-none-elf` (the config default). `blinky` is built
for real in release (it links — the dual-PAC bug is fixed and hisi-riscv-rt exports its linker
scripts to downstream bins).

## Quick commands

```bash
cargo build -Zbuild-std=core,alloc                      # default members
cargo check -Zbuild-std=core,alloc --workspace --exclude wifi_softap \
  --features hisi-rf/chip-ws63,hisi-rf/profile-wifi-wpa2-smoltcp
cargo check -Zbuild-std=core,alloc -p wifi_softap       # mutually exclusive AP archive lane
cargo clippy -Zbuild-std=core,alloc --workspace --exclude wifi_softap \
  --features hisi-rf/chip-ws63,hisi-rf/profile-wifi-wpa2-smoltcp -- -D warnings
cargo fmt --all -- --check
cargo build -Zbuild-std=core,alloc -p blinky --release
```

## Documentation

```bash
cargo doc -Zbuild-std=core,alloc -p hisi-hal -p ws63-pac -p hisi-riscv-rt --no-deps
# Output: target/riscv32imfc-unknown-none-elf/doc/hisi_hal/index.html
```

## Test

Host tests are real executable evidence, but must override the repository's default
RISC-V target. The driver derives the current rustc host tuple. Representative lanes are:

```bash
HOST_TARGET="$(rustc -vV | sed -n 's/^host: //p')"
cargo test -p hisi-hal --no-default-features --features chip-ws63 --target "$HOST_TARGET"
cargo test -p ws63-rf-rs --lib --target "$HOST_TARGET"
cargo test -p hisi-rtos --target "$HOST_TARGET"
```

Host tests prove pure logic and contract behavior only. QEMU and named WS63 HIL markers
remain separate evidence layers; do not translate a host pass into a silicon claim.

## Gotchas

- **Needs the pinned nightly + rust-src** (above). Missing `-Zbuild-std=core,alloc`
  or `rust-src` usually shows up as "can't find crate for core".
- **Single PAC instance**: the root `Cargo.toml` `[patch.crates-io]` redirects the
  `ws63-pac` registry dep to the local submodule. Don't add a second `ws63-pac` source.
- **`crates/chips/ws63/ws63-pac/src/lib.rs` is svd2rust-generated** — do not hand-edit
  it. Change the nested `ws63-svd/WS63.svd`, regenerate, and follow the
  `pac-svd-register-access` skill.
- **Submodule changes**: commit inside the submodule first, push, then bump the parent
  pointer. Use the `submodule-commit` skill.
- **`git submodule update --init --recursive`** if you get missing-manifest errors.

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `can't find crate for core` | Install `rust-src` and pass `-Zbuild-std=core,alloc` for RISC-V commands |
| `failed to load manifest for workspace member` | `git submodule update --init --recursive` |
| `error[E0463]: can't find crate for proc_macro2` (fresh CI) | Stale cross-toolchain `target/` cache — don't cache `target/` across toolchains |
| `clippy FAILED` | `cargo clippy -Zbuild-std=core,alloc --workspace -- -D warnings` to see warnings |
| `formatting FAILED` | `cargo fmt --all` to auto-fix (a PostToolUse hook also auto-formats `.rs` on edit) |
| blinky link error `__*_stack_top__` undefined | ensure hisi-riscv-rt is up to date (it exports `ws63-link.x` for downstream bins) |
