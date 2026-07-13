# hisi-riscv-rs

A Rust embedded ecosystem for the **HiSilicon RISC-V family** — the **WS63**
(Wi-Fi 6 + BLE + SLE/SparkLink) and the **BS2X** SKUs (**BS20 / BS21E / BS22**;
BLE + SLE/NearLink), all single-core RV32IMFC (hard-float `ilp32f`, no atomics) SoCs.

This monorepo bundles per-chip `svd2rust` peripheral-access crates, a hand-written
multi-chip safe HAL, a runtime, a porting layer for the closed-source Wi-Fi/BLE
blobs, and runnable examples — buildable today with the official upstream Rust
nightly target path, and
runnable **without hardware** on the sister project
[`hisi-riscv-qemu`](https://github.com/hispark-rs/hisi-riscv-qemu) (machines
`-M ws63 / bs21 / bs21e / bs22 / bs20`).

> **North star: connectivity.** Everything here is aimed at eventually bringing
> up Wi-Fi/BLE on the WS63 in Rust. **Current status (2026-07):** WS63 Wi-Fi RF porting layer + netif→smoltcp complete but pending real blob TX/RX and connectivity HIL (connectivity milestones C1-C5). BS2X BLE is deferred: the radio interface is a closed blob boundary (`0x59000000` write-only PHY regs + IRQ-26 event wall); current priorities are in [`ROADMAP.md`](ROADMAP.md), and the historical feasibility/remediation ledger is archived under [`docs/archive/`](docs/archive/). Full QEMU bring-up done for both chips; the WS63 HAL driver-level HIL evidence is tracked in [`docs/src/reference/10-stable-api.md`](docs/src/reference/10-stable-api.md), while example smoke and connectivity HIL continue separately. See [`docs/`](docs/) for the architecture (Chinese).

## Crates

Each library crate is a standalone repository (a git submodule here) and is
published independently to crates.io; `ws63-rf-rs` and `ws63-flashboot` live
in-tree and are not published.

| Crate | Role | crates.io |
|-------|------|-----------|
| [`ws63-pac`](crates/chips/ws63/ws63-pac/) | `svd2rust`-generated WS63 peripheral access (raw `RegisterBlock`s, `Peripherals::take()`) | [`ws63-pac`](https://crates.io/crates/ws63-pac) |
| [`bs2x-pac`](crates/chips/bs2x/bs2x-pac/) | `svd2rust`-generated BS21/BS2X peripheral access (the multi-chip sibling of `ws63-pac`) | — |
| [`hisi-hal`](crates/hisi-hal/) | Hand-written safe drivers on `embedded-hal 1.0` (GPIO, UART, SPI, I2C, DMA, timers, clocks, …) — plus optional `async` (`embedded-hal-async`/`embedded-io-async`) and `embassy` (an embassy-time driver). Standalone builds have no default chip: enable `chip-ws63`; experimental `chip-bs21` also requires `unstable`. | [`hisi-hal`](https://crates.io/crates/hisi-hal) |
| [`hisi-riscv-rt`](crates/hisi-riscv-rt/) | Runtime: startup assembly, linker scripts, interrupt vectors (over `riscv-rt`) | [`hisi-riscv-rt`](https://crates.io/crates/hisi-riscv-rt) |
| [`ws63-radio-sys`](crates/chips/ws63/ws63-radio-sys/) | WS63 blob ABI/archive profile and versioned `hisi-rf-link` post-link tooling; nests the language-neutral vendor payload | GitHub/submodule |
| [`ws63-rf-rs`](chips/ws63/rf/) | Porting layer + FFI for the closed Wi-Fi/BLE blobs (OSAL/OAL/FRW/HCC, scheduler, netif→smoltcp). In-tree, `publish = false` | — |
| [`ws63-flashboot`](chips/ws63/flashboot/) | Experimental bootloader (**not** secure boot). In-tree, `publish = false` | — |
| [`ws63-examples`](examples/ws63/) | Runnable WS63 examples (blinky, uart_hello, timer_irq, gpio_irq, dma_loopback, …) | — |
| [`bs21-examples`](examples/bs21/) | BS21 isolated workspace; current member list is in [`docs/src/reference/02-examples.md`](docs/src/reference/02-examples.md) | — |
| [`bs20-examples`](examples/bs20/) | BS20 isolated workspace (128K RAM variant); current member list is in [`docs/src/reference/02-examples.md`](docs/src/reference/02-examples.md) | — |

## Repository layout

The repo uses git submodules extensively. Two are **nested under the crate that
owns them**, so generation inputs / vendor blobs are not reached into laterally:

```
hisi-riscv-rs/
├── crates/                    # core publishable library crates
│   ├── pac/
│   │   ├── ws63-pac/          # submodule
│   │   │   └── ws63-svd/      # submodule of ws63-pac — svd2rust source (WS63.svd)
│   │   └── bs2x-pac/          # submodule
│   │       └── bs2x-svd/      # submodule of bs2x-pac — svd2rust source (BS2X.svd)
│   ├── hisi-hal/              # submodule (multi-chip: chip-ws63 / chip-bs21)
│   ├── hisi-riscv-rt/         # submodule
│   └── ws63-radio-sys/        # submodule: ABI/link tools
│       └── ws63-RF/           # nested vendor blob payload
├── examples/                  # application examples
│   ├── ws63/                  # submodule (blinky, uart_hello, …)
│   ├── bs21/                  # in-tree, isolated workspace (10+ examples: SPI, GADC, I2C, KEYSCAN, QDEC, RTC, WDT, DMA, USB, PDM)
│   └── bs20/                  # in-tree, isolated workspace (BS20 variant: same examples, 128K RAM)
├── chips/                     # chip-specific support
│   ├── ws63/
│   │   ├── guide/             # submodule — WS63 user guide
│   │   ├── rf/                # transitional in-tree crate (ws63-rf-rs)
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

### 1. Install the pinned official Rust nightly

The default target `riscv32imfc-unknown-none-elf` (hardware single-float
`ilp32f`, no atomics) is an upstream rustc builtin target in the pinned nightly.
rustup does not ship a prebuilt std component for it yet, so RISC-V builds use
`rust-src` plus `-Zbuild-std=core,alloc`:

```bash
rustup toolchain install nightly-2026-07-09 \
    --profile minimal \
    --component rust-src \
    --component clippy \
    --component rustfmt \
    --component llvm-tools-preview
```

### 2. Build

```bash
cargo build -Zbuild-std=core,alloc                  # libraries + default-member examples
cargo check -Zbuild-std=core,alloc --workspace      # full workspace (incl. flashboot)
cargo build -Zbuild-std=core,alloc -p blinky --release
```

Lint / format:

```bash
cargo clippy -Zbuild-std=core,alloc --workspace -- -D warnings
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

[`hisi-riscv-qemu`](https://github.com/hispark-rs/hisi-riscv-qemu) is a QEMU fork with an
in-tree WS63 machine (`-M ws63`) that models the CPU + xlinx custom ISA, memory
map, interrupt controller, and the established 35-peripheral SVD model. The PAC
now also exposes `BT_EM_CTL`; QEMU parity for that newly modeled register block
is still pending. It runs ws63-rs firmware
(and real vendor C-SDK firmware) and is the software-in-the-loop stand-in for
the current roadmap's baseline/HIL support track:

```bash
# in a sibling checkout of hisi-riscv-qemu
bash scripts/build.sh
WS63_RS=../ws63-rs bash scripts/smoke-test.sh   # boots ws63-rs examples + asserts behaviour
```

## Async & embassy

`hisi-hal` has an async layer (no heap, no global executor required), built on
`embedded-hal-async` / `embedded-io-async`. It runs on the no-atomics WS63 core via
portable-atomic + critical-section, but the public surface is deliberately split:

- **`async`** — stable blocking-backed async trait impls for SPI/I2C.
- **`async + unstable`** — interrupt/waker helpers such as `block_on`, `IrqSignal`,
  GPIO wait, timer delay, UART async I/O, and DMA/LSADC async hooks.
- **`embassy + unstable`** — the embassy-time driver and embassy examples.

See [`docs/src/reference/10-stable-api.md`](docs/src/reference/10-stable-api.md) for the current boundary.

## Releasing

Each published crate **self-publishes from its own repository**: bump + tag
`vX.Y.Z` in `ws63-pac` / `hisi-hal` / `hisi-riscv-rt`, and that repo's
`.github/workflows/release.yml` publishes it to crates.io (using its own
`CRATES_IO_TOKEN`). The monorepo `v*` tag cuts only a **firmware GitHub
Release** — it does not publish the library crates.

## Documentation

- [`CLAUDE.md`](CLAUDE.md) — build commands, architecture, design decisions.
- [`docs/src/explanation/components/01-overview.md`](docs/src/explanation/components/01-overview.md) — the whole picture (Chinese), with per-component docs alongside.
- [`docs/review/`](docs/review/) — the architecture review ledger.
- [`ROADMAP.md`](ROADMAP.md) — current connectivity-first roadmap; historical remediation details live in [`docs/archive/`](docs/archive/).
- **Open tasks:** tracked as GitHub issues on [hispark-rs/hisi-riscv-rs](https://github.com/hispark-rs/hisi-riscv-rs/issues). Probe-rs debug support (fork [hispark-rs/probe-rs](https://github.com/hispark-rs/probe-rs) branch `add-hisilicon-ws63-bs21-hil-baseline`) is in use for WS63 HIL; connectivity bring-up remains open.

## License

MIT for the Rust code (see each crate's `Cargo.toml`). The closed-source vendor
blobs under `crates/chips/ws63/ws63-radio-sys/ws63-RF` carry HiSilicon's own license and are **not**
MIT — that delivery stays language-neutral and is only linked, never modified.
