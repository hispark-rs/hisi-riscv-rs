---
name: typed-config
description: Make a hisi-riscv-hal driver's CONFIG surface "if it compiles, it runs on silicon" — design, review, or tighten a Config / new() / configure() so a value you can WRITE is a value that RUNS, with no parameter that compiles but is silently clamped, truncated, left without a clock, or dependent on an unenforced precondition. Use when adding a driver, reviewing/PR-ing a config API, tightening one for the 0.5.0 config pass, or diagnosing a "compiles but wrong/dead on hardware" bug. Applies the project's primary API convention (see CLAUDE.md "Typed config").
---

# Typed config — "if it compiles, it runs on silicon"

The HAL's **configuration** surface is typed so an out-of-range value is
*unrepresentable*, not silently mis-programmed. This is the project's primary API
convention. It adopts **esp-hal's guideline** ("prefer compile-time checks over
runtime checks; prefer a fallible API over panics"), **"parse, don't validate"**
(only fallible constructors — the value either parses or does not exist), and the
**typestate pattern** for role-dependent state.

## Two layers (never blur them)

| Layer | Who | Rule |
|-------|-----|------|
| **Config / construction** | HAL-inherent methods (`Config`, `new*`, `configure`, `set_*`) — NOT embedded-hal | **Free to type.** Validated newtypes + fallible ctors; typestate for roles; self-enable the clock gate. |
| **Operational** | embedded-hal traits (`SetDutyCycle`, `SpiBus`, `I2c`, `Read`/`Write`, `DelayNs`, `digital::Wait`) | **Fixed signatures** (`u16`/`&[u8]` + `Result`). `Result` IS embedded-hal's idiom for invalid input. **Do NOT change trait method signatures.** |

The two are almost always disjoint — the "compiles-but-won't-run" bugs live in the
config layer, which embedded-hal does not constrain.

## Defect taxonomy (classify every config field)

- **A — register-field overflow.** A computed value (divider/period/load) is wider
  than the hardware field and is silently masked/truncated (`& 0xFFFF`, `as u16`).
- **B — valid-but-dead combo.** Structurally valid config that produces no working
  clock/output (e.g. an I2S Master with zero clock dividers).
- **C — unenforced precondition.** A clock gate that must be enabled first, a board
  crystal / analog AFE that must be provisioned, an XIP-unsafe context.
- **D — silent clamp/wrap.** Out-of-range is quietly `.clamp()`ed / `saturating_`ed /
  `if x == 0 { 1 }`-ed instead of signalled.

## Decision tree — pick the tightening per field

- **Frequency / baud / period / timeout** (computed from a runtime value) → a
  **validated newtype** with `const fn try_from_hz(u32) -> Option<Self>` /
  `from_count` / `try_new`, rejecting anything outside the achievable register range.
  Reject, don't clamp. (A, D)
- **Role-dependent config** (the legal fields depend on a mode) → **type-state**: the
  state that needs extra params *requires* them in its constructor; the illegal combo
  is unrepresentable. (B)
- **Small finite choice** → an **enum** (it already can't hold an invalid value — no
  action unless it's currently a raw int).
- **Clock gate off** → the driver **self-enables its own gate** in `configure`/`new`
  (mirror the vendor `*_porting` clock-enable: CKEN + any DIV_CTL divider + LOAD_DIV).
  (C)
- **Board-population / analog precondition** that types genuinely cannot express (RTC
  32 kHz crystal, ADC AFE/LDO power-up) → **doc-and-guard**: a clearly-named/`cfg`-gated
  or feature-gated constructor, a bounded poll (never an unbounded one that can stall
  the bus), and a `# Hardware requirements` doc line. (C)
- **Genuinely full-width 32-bit register / already an enum** → **no change.** Do not
  invent constraints; only tighten real defects.

## The silicon-reality rule

**The type encodes measured hardware behavior, not the datasheet.** Canonical case:
`pwm::PwmPeriod` is a `u16` because the WS63 `pwm_freq_h` high half does **not** latch
on silicon (measured: writing `0x0001` reads back `0` even with the full clock tree
up) even though the vendor `regs_def` declares the field 32-bit. If a field's real
range is uncertain, flag it for an on-board measurement **before** fixing the type
bound — don't trust the PAC/SDK width alone.

## Procedure

1. **Scan** the driver for candidates:
   `bash .claude/skills/typed-config/scan.sh crates/hisi-riscv-hal/src/<driver>.rs`
2. **Trace** each flagged value to the register it programs; get the field's real
   width from the PAC (`crates/pac/*/src/lib.rs`) and the valid range + clock
   precondition from the vendor SDK (`fbb_ws63/.../hal_*_regs_def.h`,
   `drivers/chips/ws63/porting/*`). Cite `file:line`.
3. **Classify** (A/B/C/D) and pick the approach from the decision tree.
4. **Implement** on the config layer only; leave the embedded-hal trait impls'
   signatures untouched (return `Result` there). Mirror `pwm.rs` (`PwmPeriod`/`Duty`).
5. **Update** the host unit/property tests (the newtype's accept/reject bounds) and
   the `tests/hil.rs` test for the driver.
6. **Validate on the board** (the silicon proof — see the `run-ws63-rs` / `hil-smoke`
   skills + `hil/embedded-test-runner.sh`). Register/poll-level facts are
   board-confirmable; scope-only behavior (actual waveform) and board-population
   preconditions (RTC crystal) are not — say so.
7. **Breaking** change → batch it (one minor bump) and let `cargo-semver-checks`
   confirm the bump (see CI / `just semver`).

## Reference

`crates/hisi-riscv-hal/src/pwm.rs` — `PwmPeriod` (u16, `from_count`/`try_from_hz`),
`Duty` (0..=100), `configure` self-brings-up the clock tree, `SetDutyCycle` kept.
CLAUDE.md → "Typed config — if it compiles, it runs on silicon".
