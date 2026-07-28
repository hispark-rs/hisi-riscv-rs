# Agent Instructions

This file provides guidance to coding agents when working with code in this repository.

## Repository Overview

Adhering to the ws63-rs monorepo: a Rust embedded ecosystem for the HiSilicon WS63 RISC-V SoC (Wi-Fi 6 + SLE/SparkLink + BLE). The repo uses git submodules extensively — `crates/chips/ws63/ws63-pac`, `crates/hisi-hal`, `crates/hisi-riscv-rt`, `examples/ws63` are each standalone repos linked as submodules. Chip-specific crates, including PACs, are grouped under `crates/chips/<family>/`; chip-neutral crates remain directly under `crates/`. Generation inputs and vendor blobs are nested under their owning integration repositories: `ws63-svd` lives under `ws63-pac`, while the language-neutral `ws63-RF` payload lives under `crates/chips/ws63/ws63-radio-sys`. Always clone/update with `git submodule update --init --recursive`.

### Repository layout (grouped tree)

```
crates/      core publishable library crates
  pac/         per-chip register-access crates (svd2rust-generated)
    ws63-pac/  (submodule; nests ws63-svd)     bs2x-pac/  (submodule; nests bs2x-svd)
  hisi-hal/ (submodule)                  hisi-riscv-rt/ (submodule)
examples/    application examples
  ws63/        (= ws63-examples submodule: blinky, uart_hello, …)
  bs21/        (in-tree, isolated workspace; current members in docs reference)
  bs20/        (in-tree, isolated workspace; current members in docs reference)
chips/       chip-specific support
  ws63/        guide/ (submodule)  rf/ (transitional in-tree adapter)  flashboot/ (in-tree)
  bs2x/        guide/ (submodule)
docs/        architecture docs (Chinese)        hil/  hardware-in-the-loop scripts
```

Crate **package names are unchanged** by this grouping — `cargo build -p blinky`, `-p hisi-hal`, `-p ws63-rf-rs`, etc. all work by name; only the on-disk paths are grouped. `examples/bs21` and `examples/bs20` are separate isolated workspaces (build with `--manifest-path examples/bs21/Cargo.toml` / `examples/bs20/Cargo.toml`); their current member lists live in `docs/src/reference/02-examples.md`.

**Docs (Chinese):** the full handbook is an mdBook under [`docs/`](docs/) (build with `mdbook build docs`, serve with `mdbook serve docs`), organized by the [Diátaxis](https://diataxis.fr/) framework (tutorials / how-to / reference / explanation). The per-component architecture deep-dives now live under [`docs/src/explanation/components/`](docs/src/explanation/components/) (e.g. `overview.md` for the whole picture); the full review ledger is in [`docs/review/architecture-review-2026-05.md`](docs/review/architecture-review-2026-05.md), the archived remediation ledger is under [`docs/archive/`](docs/archive/), and the current connectivity-first plan is in [`ROADMAP.md`](ROADMAP.md). Read these before large changes — connectivity is the north star.

## Build Commands

```bash
# Builds with the official upstream Rust nightly pinned in rust-toolchain.toml.
# rustc knows riscv32imfc-unknown-none-elf (hardware single-float ilp32f, no atomics),
# but rustup does not ship its prebuilt rust-std component yet, so RISC-V commands
# use `-Zbuild-std=core,alloc`. Default target is set in .cargo/config.toml.
cargo build -Zbuild-std=core,alloc                         # Build libraries + default-member WS63 examples.
cargo check -Zbuild-std=core,alloc --workspace --features hisi-rf/chip-ws63  # Full WS63 workspace check.
# The HAL has NO default chip (esp-hal style) — building it STANDALONE needs an explicit
# chip feature, else a `compile_error!` fires:
cargo check -Zbuild-std=core,alloc -p hisi-hal --features chip-ws63    # Check HAL only (chip-ws63)
cargo check -Zbuild-std=core,alloc -p hisi-hal --no-default-features --features chip-bs21,rt,unstable   # …or BS2X
cargo check -Zbuild-std=core,alloc -p ws63-pac             # Check PAC only
cargo build -Zbuild-std=core,alloc -p blinky --release     # Build example

# Specific target override
cargo check -Zbuild-std=core,alloc --target riscv32imfc-unknown-none-elf

# Clippy & format
cargo clippy -Zbuild-std=core,alloc --target riscv32imfc-unknown-none-elf -- -D warnings
cargo fmt --all -- --check

# Submodule operations
git submodule update --init --recursive
git -C crates/hisi-hal status              # Work inside submodule
git -C crates/hisi-hal add -A && git -C crates/hisi-hal commit -m "..."
```

### Python tooling

All first-party Python is managed by [uv](https://docs.astral.sh/uv/). A Python
project uses its own `pyproject.toml` + `uv.lock` and is invoked with `uv run`;
an independent script uses the PEP 723 single-file form with
`#!/usr/bin/env -S uv run --script`. Workflows, HIL scripts, skills, and docs must
not call a system `python`/`python3` directly. Run
`uv run scripts/check-python-uv.py` before changing parent-owned Python entry
points. Independently versioned submodules own and enforce the same policy in
their repositories.

**Important:** When editing submodule files, commit inside the submodule first, then update and commit the parent repo's submodule pointer.

## Architecture

### Crate Dependency Chain

```
ws63-svd (XML) → ws63-pac (svd2rust generated, ~1.5MB lib.rs)
                → hisi-hal (hand-written safe drivers)
                → examples/ws63/* (applications)
hisi-riscv-rt (riscv-rt facade + chip startup adapters)
```

- **`ws63-pac`**: Single-file svd2rust output. Provides raw `RegisterBlock` structs for 36 peripheral/register blocks. The `Peripherals::take()` singleton pattern ensures one-time access.
- **`hisi-hal`**: 43 source files implementing safe drivers (incl. `asynch.rs` + `embassy.rs`). Depends on `embedded-hal 1.0`, `embedded-hal-nb 1.0`, `embedded-io 0.6`, `portable-atomic`; optional `async` (`embedded-hal-async`/`embedded-io-async`) + `embassy` (embassy-time driver) features.
- **`hisi-riscv-rt`**: Runtime crate — thin `riscv-rt` entry facade plus chip startup adapters. WS63 owns `asm/ws63` + `linker/ws63` startup/layout/header resources and uses `ws63-pac/rt` for `device.x`; BS2X currently reuses the legacy adapter with per-example `memory.x` and `bs2x-pac/rt` `device.x`.

### Peripheral Singleton Pattern

`crates/hisi-hal/src/peripherals.rs` defines two macros:
- `peripheral!($name, $pac_ty)` — generates a lifetime-parameterized ZST `$name<'d>` with `steal()`, `ptr()`, and an `unstable` + `unsafe` raw `register_block()` escape hatch.
- `peripherals!(...)` — generates the `Peripherals` struct with `take()` (safe) and `steal()` (unsafe).

All 35 PAC peripherals have HAL wrappers. Drivers consume their peripheral via constructor (e.g., `Watchdog::new(wdt)`).

### Driver Module Pattern

Each driver follows this structure:
```rust
pub struct DriverName<'d> { _peripheral: PeripheralType<'d> }
impl<'d> DriverName<'d> {
    pub fn new(peripheral: PeripheralType<'d>) -> Self { ... }
    fn regs(&self) -> &'static pac::RegisterBlock { unsafe { &*PeripheralType::ptr() } }
    // ... API methods
}
// embedded-hal trait impls at bottom of file
```

### Multi-instance Peripherals (UART, I2C, SPI, DMA)

Use PhantomData with type parameter to distinguish instances:
```rust
pub struct Uart<'d, T> { _peripheral: PhantomData<&'d T> }
impl<'d> Uart<'d, Uart0<'d>> { pub fn new_uart0(...) -> Self { ... } }
impl<'d> Uart<'d, Uart1<'d>> { pub fn new_uart1(...) -> Self { ... } }
```

### Sealed Traits (`private.rs`)

- `Sealed` — crate-internal supertrait preventing external implementation of GPIO signal traits such as `PeripheralInput` and `PeripheralOutput`.
- The old empty `DriverMode`/`Blocking`/`Async` marker traits and vestigial `DmaWord` marker were removed. Real async support lives behind the `async` feature, with ungraduated interrupt/waker helpers gated by `unstable`.

### Clock Architecture

WS63 peripheral clocks default to enabled out of reset. The earlier `ClockControl` / `PeripheralGuard` RAII layer had no consumers and was removed. `clock.rs` now keeps `Peripheral` + `cken_info()` as the audited peripheral → CKEN bit reference for future clock-gating code and drift checks.

### GPIO Architecture

Three driver levels:
1. **`AnyPin<'d>`** — type-erased pin wrapper. Created via `unsafe steal(pin_number)`.
2. **`Input<'d>` / `Output<'d>` / `Flex<'d>`** — typed drivers created from `AnyPin` via `init_input()`, `init_output()`, `init_flex()`.
3. **`GpioPin<'d, MODE>`** — legacy type-state GPIO (backward compatibility).

Config API: `InputConfig { pull }`, `OutputConfig { initial_high }`. The previous `open_drain` field was a no-op and was removed rather than kept as a misleading stable knob.

### DMA Architecture

Two controllers share `dma::RegisterBlock`:
- `Dma0` (0x4A00_0000) — primary DMA, channels 0-3
- `Sdma0` (0x520A_0000) — secure DMA, channels 0-3 (logical 8-11)

`DmaInstance` trait provides `ptr()` → register block access. `DmaDriver<'d, T: DmaInstance>` is generic over the controller.

**0.6.0 gate:** the public `dma` module is behind `unstable` until the safe-DMA invariants are closed (cache-line ownership/alignment, timeout quiescence, async cancellation, and SPI1/UART DMA evidence). With `unstable`, the design exposes `DmaDriver`, typed channel tokens, mem-to-mem transfers, and peripheral-paced SPI/UART DMA; these APIs remain experimental even where individual HIL tests exist. See `docs/review/peripheral-dma-design-0.5.1.md` and `docs/review/stable-api-graduation-review-2026-07-02.md`.

## Key Design Decisions

- **No `std`** — `#![no_std]` throughout. No heap, no `Vec` in driver code. Use fixed arrays when data buffers are needed.
- **Safety via lifetime generics** — peripherals are `'d`-parameterized to prevent use-after-drop of the `Peripherals` token.
- **Register access is `unsafe`** — raw PAC register writes use `unsafe { reg.write(|w| w.bits(val)) }`. Driver methods encapsulate this.
- **Async & embassy** — `async` remains a feature gate for async trait impls; the blocking-backed SPI/I2C async traits build with `async` alone, while interrupt/waker helpers (`asynch::block_on`, `IrqSignal`, GPIO wait, timer async delay, UART async I/O, DMA/LSADC async hooks) require `unstable`. `embassy` is also `unstable`-gated until end-to-end HIL. See `docs/src/explanation/components/06-async-embassy.md`.
- **SPI/I2C/UART instances use separate type constructors** — not unified `new()` because each instance may have unique configuration needs.

### Typed config — "if it compiles, it runs on silicon"

**The project's primary API convention.** The HAL's *configuration* surface is typed
so that a value you can **write** is a value that **runs** on real silicon — no
parameter that compiles but is silently clamped, truncated, or left without a clock.
This adopts esp-hal's API guideline ("prefer compile-time checks over runtime checks;
prefer a fallible API over panics"), Alexis King's "parse, don't validate", and the
typestate pattern. Two layers:

- **Config / construction — HAL-inherent, so free to type.** Use validated newtypes
  with **fallible constructors** (`try_from_hz` / `from_count` → `Option`): an
  out-of-range value returns `None` at construction, never a silent clamp/truncate.
  Role-dependent configs use **type-state** (e.g. an I2S `Master` constructor
  *requires* non-zero clock dividers — a zero-divider Master is unrepresentable). A
  driver **self-enables its own clock gate** in `configure`/`new` ("construct →
  clocked"). The type encodes **measured silicon reality, not the datasheet** — e.g.
  `pwm::PwmPeriod` is a `u16` because the WS63 `pwm_freq_h` high half does not latch
  on silicon despite the SDK declaring the field 32-bit.
- **Operational — embedded-hal traits, fixed signatures.** `SetDutyCycle` / `SpiBus`
  / `I2c` / `Read` / `Write` keep their standard `u16` / `&[u8]` + `Result`
  signatures (`Result` *is* embedded-hal's idiom for invalid input). These are NOT
  compile-time-typed; do not change trait method signatures.
- **Unsafe-adjacent identities — no raw safe inputs.** Public safe APIs must not
  accept raw `u8`/`usize`/addresses when those values select a register bank,
  DMA channel, pad, timer channel, UART port, PWM channel, or eFuse byte. Use
  typed tokens/newtypes/enums such as `DmaChannel`, `DmaTransferSize`,
  `DmaSyncMask`, `GpioPad`, `UartPad`, `MuxFunction`, `GpioBank`,
  `TimerChannel`, `UartPort`, `PwmChannelId`, and `EfuseByteAddress`.

When adding or tightening a driver, run the **`typed-config` skill** (the checklist +
the A/B/C/D defect taxonomy + a candidate scanner). Reference implementation:
`crates/hisi-hal/src/pwm.rs` (`PwmPeriod` / `Duty`). Every tightened surface is
proven on the connected board via the HIL suite (`tests/hil.rs`).

### Atomics & critical-section discipline

The product family is designed around two realistic synchronization shapes:

- **Single hart + no A extension** (WS63 current path): the target must not emit
  `lr/sc/amo*`; RMW/CAS semantics come from `portable-atomic` over
  `critical-section-single-hart`, i.e. short irq-disabled regions.
- **Multi hart + A extension** (future/other products): single-word flags/counters
  may use hardware atomics, but compound invariants still need a real lock,
  `critical-section`, or platform-level cross-hart synchronization.

Other combinations are not default product targets. In particular, disabling
interrupts on the current hart is not cross-hart mutual exclusion.

Critical sections are for **short Rust memory metadata**, not external progress or
whole peripheral transactions. Inside `critical_section::with` / irq-disabled code,
only update claim flags, refcounts, small state enums, waker slots, and IRQ
bookkeeping. Do **not** poll hardware, wait for FIFO/DMA/clock/reset, perform
SPI/UART/I2C/DMA transfers, bulk copy/cache-maintain large buffers, `delay`,
`block_on`, call user callbacks/closures/trait objects, or hold borrows across
`.await`. If a transaction needs a `Busy` invariant, set/clear that state in two
short critical sections and do the MMIO/waiting outside. ISR paths should ack /
record / wake; user logic runs later in normal context or future polling.

## CI/CD

Seven GitHub Actions workflows in `.github/workflows/`:
- `ci.yml` — main CI (build, clippy, fmt, workspace, host test, audit)
- `ci-nightly.yml` — daily nightly builds + binary size report
- `documentation.yml` — `cargo doc` build + GitHub Pages deploy + link check
- `hil.yml` — self-hosted WS63 hardware-in-the-loop suite
- `issue-handler.yml` — auto-label + first-contributor welcome
- `merge-conflict.yml` — conflict marker detection + PR labeling
- `release.yml` — parent-repo firmware GitHub Release on tag; crates.io publishing is owned by each crate repo

### Release lockfile policy

完整操作流程见 [`docs/src/how-to/11-release.md`](docs/src/how-to/11-release.md)。

Every independently published Rust repository in this ecosystem commits its own
`Cargo.lock`, including library crates. This applies to `hisi-hal`,
`hisi-riscv-rt`, `ws63-pac`, and `bs2x-pac`. Their standalone CI and `publish.yml`
must use `--locked`; release preflight is:

```bash
cargo generate-lockfile --locked
git diff --exit-code -- Cargo.lock
cargo package --locked
```

Do not rely on the parent workspace lockfile for a submodule crate release. If a
submodule lockfile changes, commit it inside that submodule before updating the
parent submodule pointer. The parent `Cargo.lock` only records the parent workspace
resolution.

## Stable / Unstable API gating (0.6.0+)

**Policy: an API is STABLE only if a named HIL test exercises it on real WS63
silicon** (the only connected board). APIs with no on-silicon test are gated
**UNSTABLE** — behind the `unstable` cargo feature (OFF by default). Adding
`features = ["unstable"]` to a consumer's `Cargo.toml` restores the experimental
surfaces; their signatures may change in a minor release.

The mechanism mirrors esp-hal: the [`instability`](https://crates.io/crates/instability)
proc-macro (`#[instability::unstable]`) soft-gates an item — `pub` when `unstable`
is on, `pub(crate)` + `#[allow(dead_code)]` when off (the item stays compiling
in-crate, so a missed stable→unstable reference doesn't break the build). Module-
level soft gating uses the crate-local `unstable_module!` macro (esp-hal form:
`pub mod` when on, `pub(crate) mod` when off; `#[doc(hidden)]`, forwards
`$(#[$meta])*` incl. `#[path]`). Standalone experimental drivers may instead use
the crate-local `unstable_driver!` hard gate (`pub mod` when on, absent when off)
when no stable code depends on the module. Both gating macros are in
`src/macros.rs` (crate-private, `#[macro_use]` — NOT `#[macro_export]`).

**Gating rules:**
- **Inherent impl blocks** stay UNGATED — gate each `pub fn` individually
  (`instability` hard-deletes `impl` blocks when off, which would make private
  helpers dead-code). `impl Drop` stays UNGATED (keeps helpers live). Trait impls
  MAY be whole-block gated.
- **STABLE pub fn taking an UNSTABLE type** as param/return is FORBIDDEN
  (`private_interfaces` lint). If a STABLE method needs an UNSTABLE type, either
  the type becomes STABLE or the method becomes UNSTABLE.
- **Standalone experimental modules** MAY use `unstable_driver!` only when default
  stable code has no dependency on that module. Otherwise use `unstable_module!`
  to keep crate-internal references compiling while hiding the external API.
- **`async`/`embassy`** are feature-gates (consent-by-feature). `embassy` is ALSO
  `unstable`-gated (no end-to-end HIL). `async` alone only exposes the blocking-
  backed SPI/I2C async trait impls; interrupt/waker-backed helpers and drivers are
  ALSO `unstable`-gated until lost-wake/cancellation invariants and HIL are closed.
- **Graduation** (unstable → stable): delete the `#[instability::unstable]` attr
  (or move the module out of `unstable_module!`) — the item was already compiling
  as `pub(crate)`, so its lint state is unchanged; residue-free. Optionally replace
  with `#[instability::stable(since = "0.x.0")]` to keep a "Stabilized in version X"
  doc note.

**Current STABLE / UNSTABLE split:** do not duplicate the inventory here. The
single source of truth is
[`docs/src/reference/10-stable-api.md`](docs/src/reference/10-stable-api.md);
this section only records the policy and mechanism. When changing public API
gates, update that reference page and the HIL evidence/review notes in the same
change.

**Release gate matrix** (verify all positive rows + clippy `-D warnings`, plus
the BS2X negative gate before changing public API gates):
`{ws63,rt}`, `{ws63,rt,unstable}`, `{ws63,rt,async,embassy}`,
`{ws63,rt,async,unstable}`, `{ws63,rt,async,embassy,unstable}`,
`{bs21,rt,unstable}`. `{bs21,rt}` without `unstable` must fail with the BS2X
experimental compile_error. BS2X isolated examples need explicit
`cargo check --manifest-path` CI checks (they're not in `cargo check --workspace`).

## Reference Material

- **esp-hal** (`/root/esp-hal/`) — reference HAL implementation. WS63 HAL patterns are modeled on esp-hal's GPIO type system, RAII clock guards, and sealed trait patterns.
- **fbb_ws63** (`/root/fbb_ws63/`) — official C SDK for WS63. Contains complete drivers, bootloader, protocol stacks (WiFi/BT/BLE/SLE/Radar), LiteOS kernel, and 13+ vendor board BSPs. Useful for verifying register behavior and peripheral configuration.
