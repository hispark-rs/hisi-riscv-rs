# ws63-rs

A Rust embedded ecosystem for the **HiSilicon WS63** — a single-core RISC-V
(RV32IMFC, hard-float `ilp32f`, no atomics) Wi-Fi 6 + BLE + SLE/SparkLink SoC.

This monorepo bundles a `svd2rust` peripheral-access crate, a hand-written safe
HAL, a runtime, a porting layer for the closed-source Wi-Fi/BLE blobs, and
runnable examples — buildable today with a custom Rust toolchain, and runnable
**without hardware** on the sister project [`ws63-qemu`](https://github.com/hispark-rs/ws63-qemu).

> **North star: connectivity.** Everything here is aimed at eventually bringing
> up Wi-Fi/BLE on the WS63 in Rust. **Current status (2026-06):** WS63 Wi-Fi RF porting layer + netif→smoltcp complete but pending real blob TX/RX and on-silicon validation (ROADMAP phase 4/5). BS2X BLE is deferred: the radio interface is a closed blob boundary (`0x59000000` write-only PHY regs + IRQ-26 event wall); full analysis in `docs/bs21-connectivity-feasibility.md`. Full QEMU bring-up done for both chips; HIL scaffolding ready. See [`ROADMAP.md`](ROADMAP.md) for the staged plan and [`docs/`](docs/) for the architecture (Chinese).

## Crates

Each library crate is a standalone repository (a git submodule here) and is
published independently to crates.io; `ws63-rf-rs` and `ws63-flashboot` live
in-tree and are not published.

| Crate | Role | crates.io |
|-------|------|-----------|
| [`ws63-pac`](crates/pac/ws63-pac/) | `svd2rust`-generated WS63 peripheral access (raw `RegisterBlock`s, `Peripherals::take()`) | [`ws63-pac`](https://crates.io/crates/ws63-pac) |
| [`bs2x-pac`](crates/pac/bs2x-pac/) | `svd2rust`-generated BS21/BS2X peripheral access (the multi-chip sibling of `ws63-pac`) | — |
| [`hisi-riscv-hal`](crates/hisi-riscv-hal/) | Hand-written safe drivers on `embedded-hal 1.0` (GPIO, UART, SPI, I2C, DMA, timers, clocks, …) — plus optional `async` (`embedded-hal-async`/`embedded-io-async`) and `embassy` (an embassy-time driver). Multi-chip: `chip-ws63` (default) / `chip-bs21` | [`hisi-riscv-hal`](https://crates.io/crates/hisi-riscv-hal) |
| [`hisi-riscv-rt`](crates/hisi-riscv-rt/) | Runtime: startup assembly, linker scripts, interrupt vectors (over `riscv-rt`) | [`hisi-riscv-rt`](https://crates.io/crates/hisi-riscv-rt) |
| [`ws63-rf-rs`](chips/ws63/rf/) | Porting layer + FFI for the closed Wi-Fi/BLE blobs (OSAL/OAL/FRW/HCC, scheduler, netif→smoltcp). In-tree, `publish = false` | — |
| [`ws63-flashboot`](chips/ws63/flashboot/) | Experimental bootloader (**not** secure boot). In-tree, `publish = false` | — |
| [`ws63-examples`](examples/ws63/) | Runnable WS63 examples (blinky, uart_hello, timer_irq, gpio_irq, dma_loopback, …) | — |
| [`bs21-examples`](examples/bs21/) | BS21 examples (blinky, uart_hello, spi_loopback, gadc_read, i2c_scan, hid_demo, clock_rng, pwm_wdt, dma_mem) — isolated workspace, builds for `-M bs21` | — |
| [`bs20-examples`](examples/bs20/) | BS20 examples (same as BS21; isolated workspace, 128K RAM variant) — builds for `-M bs20` | — |

## Repository layout

The repo uses git submodules extensively. Two are **nested under the crate that
owns them**, so generation inputs / vendor blobs are not reached into laterally:

```
hisi-riscv-rs/
├── crates/                    # core publishable library crates
│   ├── ws63-pac/              # submodule
│   │   └── ws63-svd/          # submodule of ws63-pac — svd2rust source (WS63.svd)
│   ├── bs2x-pac/              # submodule
│   │   └── bs2x-svd/          # submodule of bs2x-pac — svd2rust source (BS2X.svd)
│   ├── hisi-riscv-hal/        # submodule (multi-chip: chip-ws63 / chip-bs21)
│   └── hisi-riscv-rt/         # submodule
├── examples/                  # application examples
│   ├── ws63/                  # submodule (blinky, uart_hello, …)
│   ├── bs21/                  # in-tree, isolated workspace (10+ examples: SPI, GADC, I2C, KEYSCAN, QDEC, RTC, WDT, DMA, USB, PDM)
│   └── bs20/                  # in-tree, isolated workspace (BS20 variant: same examples, 128K RAM)
├── chips/                     # chip-specific support
│   ├── ws63/
│   │   ├── guide/             # submodule — WS63 user guide
│   │   ├── rf/                # in-tree crate (ws63-rf-rs)
│   │   │   └── ws63-RF/       # submodule — closed Wi-Fi/BLE blobs + porting contract
│   │   └── flashboot/         # in-tree crate (ws63-flashboot)
│   └── bs2x/
│       └── guide/             # submodule — BS21/BS2X user guide
├── docs/                      # architecture docs (Chinese) + review ledger
├── hil/                       # hardware-in-the-loop scripts
├── CLAUDE.md                  # build/architecture guide
└── ROADMAP.md                 # staged plan toward connectivity
```

Always clone/update with recursion:

```bash
git submodule update --init --recursive
```

## Getting started

### 1. Install the `ws63` toolchain

The default target `riscv32imfc-unknown-none-elf` (hardware single-float
`ilp32f`, no atomics) is **baked into a custom rustc** as a builtin, so no
`-Z build-std` is needed. It is not a distributable rustup channel — install +
link it first (see [`rust-toolchain.toml`](rust-toolchain.toml) and the
[hisi-riscv-rust-toolchain](https://github.com/hispark-rs/hisi-riscv-rust-toolchain) repo):

```bash
curl -fLO https://github.com/hispark-rs/hisi-riscv-rust-toolchain/releases/download/v1.96.0-1/ws63-rust-1.96.0-x86_64-unknown-linux-gnu.tar.gz
tar xzf ws63-rust-1.96.0-*.tar.gz && rustup toolchain link ws63 "$PWD/stage2"
```

### 2. Build

```bash
cargo build                  # libraries + the default-member examples
cargo check --workspace      # full workspace (incl. flashboot)
cargo build -p blinky --release
```

Lint / format:

```bash
cargo clippy --workspace
cargo fmt --all -- --check
```

### 3. Start your own project

Scaffold a fresh app outside this repo with
[`cargo generate`](https://cargo-generate.github.io/cargo-generate/) from the
[`hisi-rs-template`](https://github.com/hispark-rs/hisi-rs-template) starter — it
wires up the toolchain, target, linker scripts, a QEMU `cargo run` runner, and the
right [crates.io](https://crates.io) deps for the chip you pick:

```bash
cargo install cargo-generate
cargo generate --git https://github.com/hispark-rs/hisi-rs-template
#   chip    = ws63 | bs21 | bs21e | bs22 | bs20   (BS2X SKUs share one HAL)
#   starter = blinky | uart_hello | async (embassy; WS63 + BS2X)
```

## Run without hardware (software-in-the-loop)

[`ws63-qemu`](https://github.com/hispark-rs/ws63-qemu) is a QEMU fork with an
in-tree WS63 machine (`-M ws63`) that models the CPU + xlinx custom ISA, memory
map, interrupt controller, and all 35 SVD peripherals. It runs ws63-rs firmware
(and real vendor C-SDK firmware) and is the software-in-the-loop stand-in for
ROADMAP phase 1 "hardware bring-up":

```bash
# in a sibling checkout of ws63-qemu
bash scripts/build.sh
WS63_RS=../ws63-rs bash scripts/smoke-test.sh   # boots ws63-rs examples + asserts behaviour
```

## Async & embassy

`hisi-riscv-hal` has an interrupt + waker driven async layer (no heap, no global
executor required), built on `embedded-hal-async` / `embedded-io-async`. It runs
on the no-atomics WS63 core via the existing portable-atomic + critical-section
polyfill. Two opt-in features:

- **`async`** — `embedded_hal_async::delay::DelayNs` (on a TIMER), `digital::Wait`
  (GPIO edges), `embedded_io_async::{Read, Write}` (UART), plus a minimal `wfi`
  `block_on`. Drivers expose `on_interrupt` hooks rather than installing ISRs.
- **`embassy`** — an [`embassy-time`](https://docs.rs/embassy-time) `Driver`
  (`now()` from the TCXO 64-bit counter, alarms from a TIMER), so
  [`embassy-executor`](https://docs.rs/embassy-executor) (platform-riscv32) runs
  `Timer::after` + multi-task scheduling + the async drivers above.

Validated on ws63-qemu — see `examples/ws63/{async_delay, embassy_multitask, embassy_async_io}`.

## Releasing

Each published crate **self-publishes from its own repository**: bump + tag
`vX.Y.Z` in `ws63-pac` / `hisi-riscv-hal` / `hisi-riscv-rt`, and that repo's
`.github/workflows/release.yml` publishes it to crates.io (using its own
`CRATES_IO_TOKEN`). The monorepo `v*` tag cuts only a **firmware GitHub
Release** — it does not publish the library crates.

## Documentation

- [`CLAUDE.md`](CLAUDE.md) — build commands, architecture, design decisions.
- [`docs/architecture/overview.md`](docs/architecture/overview.md) — the whole picture (Chinese), with per-component docs alongside.
- [`docs/review/`](docs/review/) — the architecture review ledger.
- [`ROADMAP.md`](ROADMAP.md) — remediation plan and the path to connectivity.
- [`docs/bs21-connectivity-feasibility.md`](docs/bs21-connectivity-feasibility.md) — analysis of BS2X BLE radio interface constraints.
- **Open tasks:** tracked as GitHub issues on [hispark-rs/hisi-riscv-rs](https://github.com/hispark-rs/hisi-riscv-rs/issues). Probe-rs debug support (fork [hispark-rs/probe-rs](https://github.com/hispark-rs/probe-rs) branch `add-hisilicon-ws63-bs21`) is software-complete, pending on-silicon validation.

## License

MIT for the Rust code (see each crate's `Cargo.toml`). The closed-source vendor
blobs under `chips/ws63/rf/ws63-RF` carry HiSilicon's own license and are **not**
MIT — that delivery stays language-neutral and is only linked, never modified.
