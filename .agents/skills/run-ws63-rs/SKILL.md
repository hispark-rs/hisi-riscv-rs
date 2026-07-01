---
name: run-ws63-rs
description: Build, check, lint, and test the ws63-rs embedded HAL for HiSilicon WS63 (RISC-V). Use when asked to build, verify, run checks, or test hisi-riscv-hal, ws63-pac, or any crate in this workspace.
---

Paths below are relative to the repo root, a Cargo workspace with `ws63-pac`,
`hisi-riscv-hal`, `hisi-riscv-rt`, `ws63-examples/blinky`, and `ws63-flashboot`.

## Toolchain (required)

ws63-rs builds with the custom **`hisi-riscv`** toolchain: a stable rustc with the WS63
target `riscv32imfc-unknown-none-elf` (RV32IMFC, hardware single-float `ilp32f`, no
atomics) baked in as a **builtin** — so builds need **no `-Z build-std`**. The default
target is set in `.cargo/config.toml`; `rust-toolchain.toml` pins `channel = "hisi-riscv"`.

Install it first (it is not a distributable rustup channel) — extract straight into
rustup's toolchains dir, no `link` needed:

```bash
curl -fLO https://github.com/hispark-rs/hisi-riscv-rust-toolchain/releases/latest/download/hisi-riscv-rust-1.96.0-x86_64-unknown-linux-gnu.tar.gz
mkdir -p ~/.rustup/toolchains/hisi-riscv
tar xzf hisi-riscv-rust-1.96.0-*.tar.gz --strip-components=1 -C ~/.rustup/toolchains/hisi-riscv
```

The `hisi-riscv` toolchain bundles rustc, cargo, rustfmt, clippy, and rustdoc.

## Build (agent path)

```bash
bash .agents/skills/run-ws63-rs/driver.sh all      # check + fmt + clippy + blinky build
bash .agents/skills/run-ws63-rs/driver.sh check    # cargo check + doc + blinky release build
bash .agents/skills/run-ws63-rs/driver.sh fmt      # cargo fmt --check
bash .agents/skills/run-ws63-rs/driver.sh clippy   # cargo clippy
```

All steps target `riscv32imfc-unknown-none-elf` (the config default). `blinky` is built
for real in release (it links — the dual-PAC bug is fixed and hisi-riscv-rt exports its linker
scripts to downstream bins).

## Quick commands

```bash
cargo build                      # default-members: libs + blinky (uses config default target)
cargo check --workspace          # everything incl. flashboot
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
cargo build -p ws63-flashboot --release   # experimental flashboot (excluded from default build)
```

## Documentation

```bash
cargo doc -p hisi-riscv-hal -p ws63-pac -p hisi-riscv-rt --no-deps
# Output: target/riscv32imfc-unknown-none-elf/doc/hisi_riscv_hal/index.html
```

## Test

In-binary unit tests (`#[cfg(test)]`) cannot run on the host: hisi-riscv-hal contains RISC-V
inline asm (e.g. `asm!("ebreak")`), so the crate does not compile for an x86 host. They
are compile-checked only as part of `cargo check`. Running real host unit tests requires
cfg-gating the riscv asm (ROADMAP phase 2). On-silicon validation is ROADMAP phase 1.

## Gotchas

- **Needs the `hisi-riscv` toolchain** (above). A stock rustup toolchain does not have the
  `riscv32imfc` target and will fail with "target may not be installed".
- **Single PAC instance**: the root `Cargo.toml` `[patch.crates-io]` redirects the
  `ws63-pac` registry dep to the local submodule. Don't add a second `ws63-pac` source.
- **`ws63-pac/src/lib.rs` is svd2rust-generated** — do not hand-edit it (a PreToolUse
  hook blocks edits). Change `ws63-svd/WS63.svd` and regenerate (ROADMAP phase 2).
- **Submodule changes**: commit inside the submodule first, push, then bump the parent
  pointer. Use the `submodule-commit` skill.
- **`git submodule update --init --recursive`** if you get missing-manifest errors.

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `target may not be installed` / `riscv32imfc` unknown | Install + link the `hisi-riscv` toolchain (see Toolchain) |
| `failed to load manifest for workspace member` | `git submodule update --init --recursive` |
| `error[E0463]: can't find crate for proc_macro2` (fresh CI) | Stale cross-toolchain `target/` cache — don't cache `target/` across toolchains |
| `clippy FAILED` | `cargo clippy --workspace --exclude ws63-flashboot -- -D warnings` to see warnings |
| `formatting FAILED` | `cargo fmt --all` to auto-fix (a PostToolUse hook also auto-formats `.rs` on edit) |
| blinky link error `__*_stack_top__` undefined | ensure hisi-riscv-rt is up to date (it exports `ws63-link.x` for downstream bins) |
