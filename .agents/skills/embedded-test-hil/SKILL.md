---
name: embedded-test-hil
description: Create, modify, audit, build, or run on-target embedded-test HIL tests for hisi-hal and tests-hil. Use when adding HAL driver silicon evidence, writing crates/hisi-hal/tests/hil/*.rs cases, registering #[embedded_test::tests] wrappers, running hil/embedded-test-runner.sh, filtering cargo test --test hil, or checking stable API HIL coverage.
---

# Embedded-Test HIL

Use this skill for the **driver-level** HIL track: Rust `#[test]` cases run on real WS63 silicon through
`embedded-test`, semihosting, `hisi-riscv-rt`, and `hil/embedded-test-runner.sh`. Do not use it for
example UART smoke; that is `$hil-smoke`.

## First Moves

1. Read the current harness shape:
   - `crates/hisi-hal/tests/hil.rs`
   - the relevant `crates/hisi-hal/tests/hil/<driver>.rs`
   - `hil/embedded-test-runner.sh`
2. For policy/gating context, also read:
   - `docs/src/reference/10-stable-api.md`
   - `docs/src/how-to/07-run-hil-tests.md`
3. Run the inventory when auditing or after edits:

```bash
uv run .agents/skills/embedded-test-hil/scripts/hil_inventory.py
```

## Where Tests Belong

| Need | Location | Rule |
|---|---|---|
| HAL driver/API evidence | `crates/hisi-hal/tests/hil.rs` + `tests/hil/<driver>.rs` | Preferred path for stable API graduation |
| CPU/PAC/critical-section cross-cutting smoke | `tests-hil/tests/hil.rs` | Not tied to one HAL driver |
| Complete example image behavior | `hil/hil-smoke.sh` + example crate | Use `$hil-smoke`, not this skill |

## Add Or Change A HAL HIL Test

1. Put test logic in `crates/hisi-hal/tests/hil/<driver>.rs`.
2. Register the module near the other `#[path = "hil/<driver>.rs"] mod <driver>;` lines in `tests/hil.rs`.
3. Add a small wrapper inside `#[embedded_test::tests] mod tests`:

```rust
#[cfg(feature = "chip-ws63")]
#[test]
fn wdt_leak_keeps_watchdog_armed() {
    crate::wdt::wdt_leak_keeps_watchdog_armed();
}
```

4. Keep the default suite self-contained: no jumpers, external peripherals, irreversible writes, long waits, or board-population assumptions.
5. Gate non-default cases:
   - `#[cfg(feature = "unstable")]` for tests that exercise unstable API.
   - `#[cfg(feature = "hil-loopback")]` for jumper-required tests.
   - `#[cfg(feature = "hil-rtc")]` for RTC crystal tests.
6. If the test graduates an API, update `docs/src/reference/10-stable-api.md` in the same change.

For detailed patterns and anti-patterns, read [references/patterns.md](references/patterns.md).

## Commands

Build the HAL test ELF only:

```bash
cargo test -Zbuild-std=core,alloc -p hisi-hal \
    --no-default-features --features chip-ws63,rt \
    --target riscv32imfc-unknown-none-elf \
    --test hil --no-run
```

Run on real WS63:

```bash
PROBE_YAML=/path/HiSilicon_WS63.yaml \
CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/embedded-test-runner.sh \
cargo test -Zbuild-std=core,alloc -p hisi-hal \
    --no-default-features --features chip-ws63,rt \
    --target riscv32imfc-unknown-none-elf \
    --test hil
```

Filter one test:

```bash
PROBE_YAML=/path/HiSilicon_WS63.yaml \
CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/embedded-test-runner.sh \
cargo test -Zbuild-std=core,alloc -p hisi-hal \
    --no-default-features --features chip-ws63,rt \
    --target riscv32imfc-unknown-none-elf \
    --test hil -- wdt_leak_keeps_watchdog_armed
```

Run cross-cutting `tests-hil`:

```bash
PROBE_YAML=/path/HiSilicon_WS63.yaml \
CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/embedded-test-runner.sh \
cargo test -Zbuild-std=core,alloc -p tests-hil --target riscv32imfc-unknown-none-elf
```

## Review Checklist

- The test name describes the silicon fact being proven.
- The helper file owns the MMIO details; the wrapper only registers the test.
- Unsafe singleton steals have a tight `SAFETY:` comment and leave the peripheral in a known state.
- Critical sections, ISRs, and callbacks are not used for waits or user logic.
- `--test hil` is used; bare `cargo test --target riscv32imfc...` builds the host libtest target and is wrong here.
- Docs and stable/unstable gating agree with the HIL evidence.
