# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository Overview

Adhering to the ws63-rs monorepo: a Rust embedded ecosystem for the HiSilicon WS63 RISC-V SoC (Wi-Fi 6 + SLE/SparkLink + BLE). The repo uses git submodules extensively — `crates/pac/ws63-pac`, `crates/hisi-riscv-hal`, `crates/hisi-riscv-rt`, `examples/ws63` are each standalone repos linked as submodules (the chip-specific PAC crates are grouped under `crates/pac/`). Two are **nested under the crate/dir that owns them** (so generation inputs / vendor blobs are not reached into laterally): `ws63-svd` is a submodule of `ws63-pac` (`crates/pac/ws63-pac/ws63-svd`, the svd2rust source), and `ws63-RF` is a submodule whose path lives inside the in-tree RF crate (`chips/ws63/rf/ws63-RF`, the closed Wi-Fi/BLE blobs). Always clone/update with `git submodule update --init --recursive`.

### Repository layout (grouped tree)

```
crates/      core publishable library crates
  pac/         per-chip register-access crates (svd2rust-generated)
    ws63-pac/  (submodule; nests ws63-svd)     bs2x-pac/  (submodule; nests bs2x-svd)
  hisi-riscv-hal/ (submodule)                  hisi-riscv-rt/ (submodule)
examples/    application examples
  ws63/        (= ws63-examples submodule: blinky, uart_hello, …)
  bs21/        (in-tree, isolated workspace: BS21 blinky + uart_hello)
  bs20/        (in-tree, isolated workspace: BS20 blinky + uart_hello)
chips/       chip-specific support
  ws63/        guide/ (submodule)  rf/ (in-tree, nests ws63-RF)  flashboot/ (in-tree)
  bs2x/        guide/ (submodule)
docs/        architecture docs (Chinese)        hil/  hardware-in-the-loop scripts
```

Crate **package names are unchanged** by this grouping — `cargo build -p blinky`, `-p hisi-riscv-hal`, `-p ws63-rf-rs`, etc. all work by name; only the on-disk paths are grouped. `examples/bs21` is a separate isolated workspace (build with `--manifest-path examples/bs21/Cargo.toml`).

**Docs (Chinese):** the full handbook is an mdBook under [`docs/`](docs/) (build with `mdbook build docs`, serve with `mdbook serve docs`), organized by the [Diátaxis](https://diataxis.fr/) framework (tutorials / how-to / reference / explanation). The per-component architecture deep-dives now live under [`docs/src/explanation/components/`](docs/src/explanation/components/) (e.g. `overview.md` for the whole picture); the full review ledger is in [`docs/review/architecture-review-2026-05.md`](docs/review/architecture-review-2026-05.md), and the remediation plan in [`ROADMAP.md`](ROADMAP.md). Read these before large changes — they record known defects and the intended direction (connectivity is the north star).

## Build Commands

```bash
# Builds with the custom `hisi-riscv` toolchain (rust-toolchain.toml): stable rustc with the
# WS63 target riscv32imfc-unknown-none-elf (hardware single-float ilp32f, no atomics)
# baked in as a builtin — default target set in .cargo/config.toml, no -Z build-std.
# Install it first (see rust-toolchain.toml / https://github.com/hispark-rs/hisi-riscv-rust-toolchain):
#   curl -fLO https://github.com/hispark-rs/hisi-riscv-rust-toolchain/releases/latest/download/hisi-riscv-rust-1.96.0-x86_64-unknown-linux-gnu.tar.gz
#   mkdir -p ~/.rustup/toolchains/hisi-riscv && tar xzf hisi-riscv-rust-1.96.0-*.tar.gz --strip-components=1 -C ~/.rustup/toolchains/hisi-riscv
cargo build                         # Build libraries + blinky (default-members) — works:
                                    # the default-member ws63 examples pull chip-ws63 onto
                                    # the shared hal via feature unification.
cargo check --workspace             # Full workspace check — also works (unifies chip-ws63).
# The HAL has NO default chip (esp-hal style) — building it STANDALONE needs an explicit
# chip feature, else a `compile_error!` fires:
cargo check -p hisi-riscv-hal --features chip-ws63    # Check HAL only (chip-ws63)
cargo check -p hisi-riscv-hal --no-default-features --features chip-bs21,rt   # …or BS2X
cargo check -p ws63-pac             # Check PAC only
cargo build -p blinky --release     # Build example

# Specific target override
cargo check --target riscv32imfc-unknown-none-elf

# Clippy & format
cargo clippy --target riscv32imfc-unknown-none-elf
cargo fmt --all -- --check

# Submodule operations
git submodule update --init --recursive
git -C crates/hisi-riscv-hal status              # Work inside submodule
git -C crates/hisi-riscv-hal add -A && git -C crates/hisi-riscv-hal commit -m "..."
```

**Important:** When editing submodule files, commit inside the submodule first, then update and commit the parent repo's submodule pointer.

## Architecture

### Crate Dependency Chain

```
ws63-svd (XML) → ws63-pac (svd2rust generated, ~1.5MB lib.rs)
                → hisi-riscv-hal (hand-written safe drivers)
                → examples/ws63/* (applications)
hisi-riscv-rt (startup, linker scripts, interrupt vectors)
```

- **`ws63-pac`**: Single-file svd2rust output. Provides raw `RegisterBlock` structs for all 35 peripherals. The `Peripherals::take()` singleton pattern ensures one-time access.
- **`hisi-riscv-hal`**: 43 source files implementing safe drivers (incl. `asynch.rs` + `embassy.rs`). Depends on `embedded-hal 1.0`, `embedded-hal-nb 1.0`, `embedded-io 0.6`, `portable-atomic`; optional `async` (`embedded-hal-async`/`embedded-io-async`) + `embassy` (embassy-time driver) features.
- **`hisi-riscv-rt`**: Runtime crate — startup assembly, linker scripts (memory.x, layout.ld), interrupt vector definitions (device.x). Uses `riscv-rt` underneath.

### Peripheral Singleton Pattern

`crates/hisi-riscv-hal/src/peripherals.rs` defines two macros:
- `peripheral!($name, $pac_ty)` — generates a lifetime-parameterized ZST `$name<'d>` with `steal()`, `ptr()`, `register_block()`.
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

- `Sealed` — supertrait preventing external implementation of `DmaWord`, `PeripheralInput`, `PeripheralOutput`.
- (The old empty `DriverMode`/`Blocking`/`Async` marker traits were removed; real async now lives behind the `async`/`embassy` features — see "Async & embassy" below and `docs/src/explanation/components/06-async-embassy.md`.)

### Clock Architecture

`ClockControl` wraps `CldoCrg` (clock and reset generator). Two access patterns:
1. Direct methods: `enable_uart()`, `enable_spi()`, etc.
2. RAII guards: `PeripheralGuard` with reference counting via `AtomicU8` static array. Guard enables clock on creation, decrements refcount on drop.

`Peripheral` enum maps each peripheral to `(cken_register_index, bit_position)` for hardware clock gate control.

### GPIO Architecture

Three driver levels:
1. **`AnyPin<'d>`** — type-erased pin wrapper. Created via `unsafe steal(pin_number)`.
2. **`Input<'d>` / `Output<'d>` / `Flex<'d>`** — typed drivers created from `AnyPin` via `init_input()`, `init_output()`, `init_flex()`.
3. **`GpioPin<'d, MODE>`** — legacy type-state GPIO (backward compatibility).

Config API: `InputConfig { pull }`, `OutputConfig { open_drain, initial_high }`.

### DMA Architecture

Two controllers share `dma::RegisterBlock`:
- `Dma0` (0x4A00_0000) — primary DMA, channels 0-3
- `Sdma0` (0x520A_0000) — secure DMA, channels 0-3 (logical 8-11)

`DmaInstance` trait provides `ptr()` → register block access. `DmaDriver<'d, T: DmaInstance>` is generic over the controller.

**Peripheral-paced DMA (0.5.1+):** wire DMA into a peripheral via `Spi::with_dma(dma) -> SpiDma` / `Uart::with_dma(dma) -> UartDma` (consumes the blocking driver — blocking + DMA APIs are mutually exclusive, esp-hal style). `SpiDma::{write_dma, transfer_dma}` / `UartDma::{write_dma, read_dma}` are blocking, bounded-wait, and program the peripheral + DMA channel in the vendor handshake order. Channels come from `DmaDriver::split_channels() -> DmaChannels` (typed tokens, runtime-claimed). The low-level `start_mem_to_peripheral`/`start_peripheral_to_mem` return a `PeripheralTransfer<'d, BUF>` guard that owns the buffer (UAF unrepresentable in safe code); `wait()` is fallible (`Err(Timeout)` on a wedged channel). Cache maintenance is folded in (clean TX source / invalidate RX dst; never touch the uncached peripheral MMIO). `Drop` runs cancel-then-quiesce (clear peripheral DMA-enable → halt → drain `active` → disable). See `docs/review/peripheral-dma-design-0.5.1.md`. (The old `DmaEligible`/`DmaChannelFor` binding traits were removed; `DmaPeripheral` + `DmaChannelConfig::mem_to_peripheral`/`peripheral_to_mem` replace them.)

## Key Design Decisions

- **No `std`** — `#![no_std]` throughout. No heap, no `Vec` in driver code. Use fixed arrays when data buffers are needed.
- **Safety via lifetime generics** — peripherals are `'d`-parameterized to prevent use-after-drop of the `Peripherals` token.
- **Register access is `unsafe`** — raw PAC register writes use `unsafe { reg.write(|w| w.bits(val)) }`. Driver methods encapsulate this.
- **Async & embassy** — beyond the blocking drivers, hisi-riscv-hal has an `async` feature (interrupt + waker driven `embedded-hal-async`/`embedded-io-async`: `DelayNs`, `digital::Wait`, `SpiBus`, `I2c`, `Read`/`Write`, plus `asynch::block_on` + `IrqSignal` + per-driver `on_interrupt`) and an `embassy` feature (an embassy-time `Driver` so `embassy-executor` platform-riscv32 runs `Timer::after`). Both work on the no-atomics WS63 via portable-atomic + critical-section. See `docs/src/explanation/components/06-async-embassy.md`.
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

When adding or tightening a driver, run the **`typed-config` skill** (the checklist +
the A/B/C/D defect taxonomy + a candidate scanner). Reference implementation:
`crates/hisi-riscv-hal/src/pwm.rs` (`PwmPeriod` / `Duty`). Every tightened surface is
proven on the connected board via the HIL suite (`tests/hil.rs`).

## CI/CD

Seven GitHub Actions workflows in `.github/workflows/`:
- `ci.yml` — main CI (build, clippy, fmt, workspace, host test, audit)
- `ci-nightly.yml` — daily nightly builds + binary size report
- `documentation.yml` — `cargo doc` build + GitHub Pages deploy + link check
- `issue-handler.yml` — auto-label + first-contributor welcome
- `merge-conflict.yml` — conflict marker detection + PR labeling
- `release.yml` — GitHub Release on tag + crates.io publish
- `dependabot.yml` — weekly Cargo + monthly Actions updates

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
level gating uses the crate-local `unstable_module!` macro (esp-hal form:
`pub mod` when on, `pub(crate) mod` when off; `#[doc(hidden)]`, forwards
`$(#[$meta])*` incl. `#[path]`). Both are in `src/macros.rs` (crate-private,
`#[macro_use]` — NOT `#[macro_export]`).

**Gating rules:**
- **Inherent impl blocks** stay UNGATED — gate each `pub fn` individually
  (`instability` hard-deletes `impl` blocks when off, which would make private
  helpers dead-code). `impl Drop` stays UNGATED (keeps helpers live). Trait impls
  MAY be whole-block gated.
- **STABLE pub fn taking an UNSTABLE type** as param/return is FORBIDDEN
  (`private_interfaces` lint). If a STABLE method needs an UNSTABLE type, either
  the type becomes STABLE or the method becomes UNSTABLE.
- **`async`/`embassy`** are feature-gates (consent-by-feature); `embassy` is ALSO
  `unstable`-gated (no end-to-end HIL). `async` stays STABLE (HIL-verified).
- **Graduation** (unstable → stable): delete the `#[instability::unstable]` attr
  (or move the module out of `unstable_module!`) — the item was already compiling
  as `pub(crate)`, so its lint state is unchanged; residue-free. Optionally replace
  with `#[instability::stable(since = "0.x.0")]` to keep a "Stabilized in version X"
  doc note.

**What's STABLE (HIL-proven on WS63 silicon — ungated):** gpio, spi (blocking),
uart (blocking), timer, tcxo, pwm, wdt, dma (mem-to-mem: `Dma0`/`DmaDriver`/
`Transfer`/`start_mem_to_mem`/`DmaChannelConfig`+`configure_channel`), trng (WS63),
efuse, clock, system, peripherals, interrupt, i2c (WS63 v150), i2s, io_config,
lsadc, tsensor, cache, asynch (`block_on`/`IrqSignal`), + the peripheral-DMA
HIL-proven subset (`SpiDma::write_dma`/`transfer_dma`/`write_dma_async`,
`with_dma`/`split_channels`/`DmaChannel`/`DmaPeripheral`). Plus infrastructure:
time, prelude, private, macros, soc.

**What's UNSTABLE (no on-silicon HIL — gated):** peripheral-DMA unproven subset
(`SpiDma::transfer_dma_async`/`release`, `UartDma` all, `PeripheralTransfer`,
`start_mem_to_peripheral`/`start_peripheral_to_mem`, the 4 `DmaChannelConfig`
builders, `DmaFrame`/`PeriKind`/`PeriDmaCtl`/`DmaError`), `embassy`, WS63 untested
drivers (`clock_init`/`km`/`pke`/`safety`/`sfc`/`spacc`/`ulp_gpio`/`rtc`-WS63/
`delay`), entire BS2X-specific series (`gadc`/`keyscan`/`pdm`/`qdec`/`usb`/
`i2c`-v151/`rtc`-v150/`trng`-v1 — no BS2X silicon board, QEMU only), + the
`prelude` re-exports of `sfc::SfcDriver` and `ulp_gpio::UlpGpioPin`.

**Build matrix** (CI must verify all 7 rows + clippy `-D warnings`):
`{ws63,rt}`, `{ws63,rt,unstable}`, `{ws63,rt,async,embassy}`,
`{ws63,rt,async,unstable}`, `{ws63,rt,async,embassy,unstable}`, `{bs21,rt}`,
`{bs21,rt,unstable}`. BS2X isolated examples that import UNSTABLE modules need
explicit `cargo check --manifest-path` CI checks (they're not in `cargo check
--workspace`).

## Reference Material

- **esp-hal** (`/root/esp-hal/`) — reference HAL implementation. WS63 HAL patterns are modeled on esp-hal's GPIO type system, RAII clock guards, and sealed trait patterns.
- **fbb_ws63** (`/root/fbb_ws63/`) — official C SDK for WS63. Contains complete drivers, bootloader, protocol stacks (WiFi/BT/BLE/SLE/Radar), LiteOS kernel, and 13+ vendor board BSPs. Useful for verifying register behavior and peripheral configuration.
