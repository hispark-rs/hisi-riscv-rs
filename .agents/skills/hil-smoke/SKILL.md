---
name: hil-smoke
description: Build a ws63-rs example for a HiSilicon chip, flash it to a REAL board via hisiflash, read UART, and assert the bring-up marker — the silicon twin of qemu-smoke. Use to validate firmware on hardware (real clocks/timing/peripherals that QEMU can't prove), or run --preflight to check the HIL rig with no board attached.
disable-model-invocation: true
---

The hardware-in-the-loop counterpart to `qemu-smoke`: same chip→example model, but it
flashes silicon and reads the board's UART instead of booting QEMU. It validates exactly
what emulation can't — the real 24 MHz TCXO timer, the 160 MHz UART baud base, real
peripheral timing (see the bring-up table in `hil/README.md`). User-invoked: it writes
firmware to hardware.

> **Status**: the `hil/` layer is scaffolding until first-board bring-up — `PORT`,
> `LOADERBOOT`, and `ADDRESS` get pinned against the real board then. Run `--preflight`
> anytime (no board needed) to see what's still missing.

## Usage

```bash
bash .Codex/skills/hil-smoke/hil.sh <chip> [example] [--preflight]
#   chip:    ws63 | bs21 | bs21e | bs22 | bs20
#   example: uart_hello | timer_irq | gpio_irq | reset_demo | spi_loopback | i2c_scan | …
```

- **Preflight** (no board / no writes): checks toolchain, hisiflash, serial port,
  loaderboot, address — reports READY / NOT-READY.
  ```bash
  bash .Codex/skills/hil-smoke/hil.sh ws63 --preflight
  ```
- **Single example**: build → flash → read UART → assert the chip-aware marker.
  ```bash
  PORT=/dev/ttyUSB0 LOADERBOOT=/path/loaderboot.bin ADDRESS=0x200000 \
    bash .Codex/skills/hil-smoke/hil.sh ws63 uart_hello
  ```
- **Full suite**: WS63 delegates to the in-tree `hil/hil-smoke.sh` (source of truth);
  BS2X runs the chip-aware checks inline (`hil-smoke.sh` is WS63-only).

## What it knows (so you don't have to)

- **Build split** — identical to `qemu-smoke`: WS63 examples are root-workspace members
  (`cargo build -p <ex>`); BS2X examples are isolated workspaces (`--manifest-path`).
  `bs21e`/`bs22` reuse the `examples/bs21` binaries (banner says **BS21**); `bs20` is its
  own dir.
- **Serial autodetect** — if exactly one `/dev/ttyUSB*`/`ttyACM*`, uses it; else set `PORT`.
- **LOADERBOOT autodiscovery** — finds a vendor `*loaderboot*.bin` under `/root/fbb_ws63`
  (WS63) or `/root/fbb_bs2x` (BS2X), skipping signed variants; override with `LOADERBOOT=`.
- **Markers** (mirror `hil/hil-smoke.sh`, chip-aware banner):

  | example | passes when |
  |---------|-------------|
  | `uart_hello` | `Hello from <WS63\|BS21\|BS20>` (validates the **160 MHz baud base**) |
  | `timer_irq` | `timer irq #` / `OK: timer` (validates the **24 MHz TCXO timer** — 10× off ⇒ still on 240 MHz) |
  | `gpio_irq` | `gpio irq #` |
  | `reset_demo` | `reset_reason=Software` |
  | `spi_loopback` | `SPI loopback OK` (**short MOSI↔MISO first**) |
  | `i2c_scan` | `scan done` / `no devices` |
  | `blinky` | no UART — verify with an LED / logic analyzer |

## Env overrides

| var | default | purpose |
|-----|---------|---------|
| `PORT` | autodetect one ttyUSB*/ttyACM* | board UART0 serial port |
| `LOADERBOOT` | autodiscover from the SDK | vendor loaderboot.bin (pushed before the program) |
| `ADDRESS` | `0x200000` | program flash offset — **verify against the partition table** |
| `BAUD` | hisiflash 921600 | flash baud |
| `UART_BAUD` | `115200` | the example's UART0 baud |
| `SETTLE` | `4` | seconds to read UART after each flash |
| `MONITOR` | raw `cat $PORT` | command that prints raw UART (for non-standard adapters) |
| `HISIFLASH` | `hisiflash` | the flash CLI binary |

## Relationship to the other skills

- `qemu-smoke` = software-in-the-loop (boot in QEMU). `hil-smoke` = hardware-in-the-loop
  (flash silicon). Same chip/example surface so results line up.
- `qemu-vs-hil` runs **both** and diffs the markers — the QEMU↔silicon parity check.
- On a HIL failure, hand the captured UART + the example to the **`hil-triage`** subagent.

## Gotchas

- **No board ⇒ can't flash.** Use `--preflight`; the real path fails fast with a clear
  message if the port/loaderboot/hisiflash are missing.
- **BS2X HIL is unverified** — the QEMU path is solid; on-silicon BS21/BS20 awaits a board.
- `ADDRESS` is a flash *offset*, not the XIP base — wrong value can misflash. Verify it.
- The actual flash goes through `hil/flash.sh` with `HIL_CONFIRM=1` (the flash-guard hook
  blocks unconfirmed `hisiflash write-program` run via Codex's Bash — see `.Codex/settings.json`).
- A first board may need `cargo install hisiflash-cli` and `gdb-multiarch` (see `hil/README.md`).
