---
name: run-ws63-rs
description: Build, check, lint, and test the ws63-rs embedded HAL for HiSilicon WS63 (RISC-V). Use when asked to build, verify, run checks, or test ws63-hal, ws63-pac, or any crate in this workspace.
---

Paths below are relative to the repo root, which is a Cargo workspace with
`ws63-pac`, `ws63-hal`, `ws63-rt`, and `ws63-examples/*`.

## Prerequisites

```bash
rustup target add riscv32imafc-unknown-none-elf
```

The `cargo fmt` check also needs `rustfmt`:

```bash
rustup component add rustfmt clippy
```

## Build (agent path)

Run the driver script — it checks compilation, docs, formatting, and
clippy in sequence:

```bash
bash .claude/skills/run-ws63-rs/driver.sh all
```

Individual steps:

```bash
bash .claude/skills/run-ws63-rs/driver.sh check   # cargo check + doc + size check
bash .claude/skills/run-ws63-rs/driver.sh fmt     # cargo fmt --check
bash .claude/skills/run-ws63-rs/driver.sh clippy  # cargo clippy
```

All steps target `riscv32imafc-unknown-none-elf`. The `check` step also
performs release-profile size checks on the blinky example (compile-only,
no linking — linking requires matching LLVM bitcode versions).

## Quick single-crate check

```bash
cargo check -p ws63-hal --target riscv32imafc-unknown-none-elf
cargo check -p ws63-pac --target riscv32imafc-unknown-none-elf
cargo check -p ws63-rt --target riscv32imafc-unknown-none-elf
cargo check -p blinky --target riscv32imafc-unknown-none-elf
```

## Build documentation

```bash
cargo doc -p ws63-hal --target riscv32imafc-unknown-none-elf --no-deps
# Output: target/riscv32imafc-unknown-none-elf/doc/ws63_hal/index.html
```

## Test

In-binary unit tests (bare-metal `#[cfg(test)]` blocks) cannot run on
Linux — they require a RISC-V target with the `test` crate, which is
only available on bare-metal or QEMU targets. Compile-check only:

```bash
cargo check --tests -p ws63-hal --target riscv32imafc-unknown-none-elf
# Note: will show "can't find crate for `test`" — expected for bare-metal
```

For logic-only tests (time.rs Duration/Rate arithmetic), those are
verified at compile time through `cargo check` since they live inside
`#[cfg(test)]` modules. They cannot be executed on the host.

## Gotchas

- **`cargo build --release` may fail with LLVM bitcode errors** on the
  blinky example — this is a rustc/LLVM version mismatch with the
  ws63-pac precompiled crate. Use `cargo check --release` instead for
  size verification.
- **`cargo test` cannot run on the host** — the RISC-V bare-metal
  target has no `test` crate. Tests are compile-check only.
- **The workspace uses git submodules** (`ws63-pac`, `ws63-hal`,
  `ws63-rt`, `ws63-examples`). If you get missing-package errors, run
  `git submodule update --init --recursive`.
- **Changing ws63-hal submodule files** requires committing inside the
  submodule first, then updating the parent repo's submodule pointer.

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `failed to load manifest for workspace member` | `git submodule update --init` |
| `can't find crate for 'ws63_pac'` | The PAC is a git dependency; `cargo update` |
| `error: could not compile 'blinky'` (bitcode) | Use `cargo check` instead of `cargo build` |
| `clippy FAILED` | Run `cargo clippy -p ws63-hal --target riscv32imafc-unknown-none-elf` to see warnings |
| `formatting FAILED` | Run `cargo fmt --all` to auto-fix |
