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
chips/       chip-specific support
  ws63/        guide/ (submodule)  rf/ (in-tree, nests ws63-RF)  flashboot/ (in-tree)
  bs2x/        guide/ (submodule)
docs/        architecture docs (Chinese)        hil/  hardware-in-the-loop scripts
```

Crate **package names are unchanged** by this grouping — `cargo build -p blinky`, `-p hisi-riscv-hal`, `-p ws63-rf-rs`, etc. all work by name; only the on-disk paths are grouped. `examples/bs21` is a separate isolated workspace (build with `--manifest-path examples/bs21/Cargo.toml`).

**Architecture docs (Chinese):** see [`docs/`](docs/) — [`docs/architecture/overview.md`](docs/architecture/overview.md) for the whole picture, per-component docs under `docs/architecture/`, the full review ledger in [`docs/review/architecture-review-2026-05.md`](docs/review/architecture-review-2026-05.md), and the remediation plan in [`ROADMAP.md`](ROADMAP.md). Read these before large changes — they record known defects and the intended direction (connectivity is the north star).

## Build Commands

```bash
# Builds with the custom `ws63` toolchain (rust-toolchain.toml): stable rustc with the
# WS63 target riscv32imfc-unknown-none-elf (hardware single-float ilp32f, no atomics)
# baked in as a builtin — default target set in .cargo/config.toml, no -Z build-std.
# Install it first (see rust-toolchain.toml / https://github.com/sanchuanhehe/ws63-rust-toolchain):
#   curl -fLO https://github.com/sanchuanhehe/ws63-rust-toolchain/releases/download/v1.96.0-1/ws63-rust-1.96.0-x86_64-unknown-linux-gnu.tar.gz
#   tar xzf ws63-rust-1.96.0-*.tar.gz && rustup toolchain link ws63 "$PWD/stage2"
cargo build                         # Build libraries + blinky (default-members)
cargo check --workspace             # Full workspace check (incl. flashboot)
cargo check -p hisi-riscv-hal             # Check HAL only
cargo check -p ws63-pac             # Check PAC only
cargo build -p blinky --release     # Build example

# Specific target override
cargo check --target riscv32imafc-unknown-none-elf

# Clippy & format
cargo clippy --target riscv32imafc-unknown-none-elf
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
- **`hisi-riscv-hal`**: 35 source files implementing safe drivers (incl. `asynch.rs` + `embassy.rs`). Depends on `embedded-hal 1.0`, `embedded-hal-nb 1.0`, `embedded-io 0.6`, `portable-atomic`; optional `async` (`embedded-hal-async`/`embedded-io-async`) + `embassy` (embassy-time driver) features.
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
- (The old empty `DriverMode`/`Blocking`/`Async` marker traits were removed; real async now lives behind the `async`/`embassy` features — see "Async & embassy" below and `docs/architecture/async-embassy.md`.)

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

`DmaInstance` trait provides `ptr()` → register block access. `DmaDriver<'d, T: DmaInstance>` is generic over the controller. `DmaEligible` trait binds peripherals to DMA, with `DmaChannelFor<P>` providing compile-time safety.

## Key Design Decisions

- **No `std`** — `#![no_std]` throughout. No heap, no `Vec` in driver code. Use fixed arrays when data buffers are needed.
- **Safety via lifetime generics** — peripherals are `'d`-parameterized to prevent use-after-drop of the `Peripherals` token.
- **Register access is `unsafe`** — raw PAC register writes use `unsafe { reg.write(|w| w.bits(val)) }`. Driver methods encapsulate this.
- **Async & embassy** — beyond the blocking drivers, hisi-riscv-hal has an `async` feature (interrupt + waker driven `embedded-hal-async`/`embedded-io-async`: `DelayNs`, `digital::Wait`, `SpiBus`, `I2c`, `Read`/`Write`, plus `asynch::block_on` + `IrqSignal` + per-driver `on_interrupt`) and an `embassy` feature (an embassy-time `Driver` so `embassy-executor` platform-riscv32 runs `Timer::after`). Both work on the no-atomics WS63 via portable-atomic + critical-section. See `docs/architecture/async-embassy.md`.
- **SPI/I2C/UART instances use separate type constructors** — not unified `new()` because each instance may have unique configuration needs.

## CI/CD

Seven GitHub Actions workflows in `.github/workflows/`:
- `ci.yml` — main CI (build, clippy, fmt, workspace, host test, audit)
- `ci-nightly.yml` — daily nightly builds + binary size report
- `documentation.yml` — `cargo doc` build + GitHub Pages deploy + link check
- `issue-handler.yml` — auto-label + first-contributor welcome
- `merge-conflict.yml` — conflict marker detection + PR labeling
- `release.yml` — GitHub Release on tag + crates.io publish
- `dependabot.yml` — weekly Cargo + monthly Actions updates

## Reference Material

- **esp-hal** (`/root/esp-hal/`) — reference HAL implementation. WS63 HAL patterns are modeled on esp-hal's GPIO type system, RAII clock guards, and sealed trait patterns.
- **fbb_ws63** (`/root/fbb_ws63/`) — official C SDK for WS63. Contains complete drivers, bootloader, protocol stacks (WiFi/BT/BLE/SLE/Radar), LiteOS kernel, and 13+ vendor board BSPs. Useful for verifying register behavior and peripheral configuration.
