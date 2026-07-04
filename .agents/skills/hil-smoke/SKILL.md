---
name: hil-smoke
description: Build a ws63-rs example for a HiSilicon chip, flash it to a REAL board via the repo HIL flash path, read UART, and assert the example marker — the silicon twin of qemu-smoke. Use for example-level hardware smoke; HAL driver-level evidence uses embedded-test via hil/embedded-test-runner.sh instead.
disable-model-invocation: true
---

The hardware-in-the-loop counterpart to `qemu-smoke`: same chip→example model, but it
flashes silicon and reads the board's UART instead of booting QEMU. It validates exactly
what emulation can't at the example-image level: real boot, UART, timing, and board
wiring. User-invoked: it writes firmware to hardware.

> **Scope**: this skill is only the UART/example smoke track. The HAL stable API proof
> track is `cargo test -p hisi-riscv-hal --test hil` through `hil/embedded-test-runner.sh`
> (`embedded-test` + semihosting), documented in `docs/src/how-to/07-run-hil-tests.md`.

## Usage

```bash
bash .agents/skills/hil-smoke/hil.sh <chip> [example] [--preflight]
#   chip:    ws63 (BS2X HIL currently reports NOT-READY; use QEMU until a rig exists)
#   example: uart_hello | timer_irq | gpio_irq | reset_demo | spi_loopback | i2c_scan | …
```

- **Preflight** (no board / no writes): checks toolchain, hisiflash, serial port,
  loaderboot, address — reports READY / NOT-READY.
  ```bash
  bash .agents/skills/hil-smoke/hil.sh ws63 --preflight
  ```
- **Single example**: build → flash → read UART → assert the chip-aware marker.
  ```bash
  PORT=/dev/ttyUSB0 PROBE_RS_YAML=/path/HiSilicon_WS63.yaml \
    bash .agents/skills/hil-smoke/hil.sh ws63 uart_hello
  ```
- **Full suite**: WS63 delegates to the in-tree `hil/hil-smoke.sh` (source of truth);
  BS2X runs the chip-aware checks inline (`hil-smoke.sh` is WS63-only).

## What it knows (so you don't have to)

- **Build split** — identical to `qemu-smoke`: WS63 examples are root-workspace members
  (`cargo build -p <ex>`); BS2X examples are isolated workspaces (`--manifest-path`).
  BS2X examples remain QEMU/build targets until a chip-specific HIL rig exists.
- **Serial autodetect** — if exactly one `/dev/ttyUSB*`/`ttyACM*`, uses it; else set `PORT`.
- **LOADERBOOT autodiscovery** — finds a vendor `*loaderboot*.bin` under `/root/fbb_ws63`
  (WS63) or `/root/fbb_bs2x` (BS2X), skipping signed variants; override with `LOADERBOOT=`.
- **Markers** — WS63 full-suite matching delegates to `hil/hil-smoke.sh`, which is
  documented in `docs/src/reference/07-hil-markers.md`. Single-example runs use
  this skill's `marker_for()` helper for chip-aware banners; keep it in sync with
  `hil/hil-smoke.sh` via
  `.agents/skills/embedded-test-hil/scripts/check_hil_smoke_markers.py`.
  `blinky` has no UART marker; verify it with an LED or logic analyzer.

## Env overrides

| var | default | purpose |
|-----|---------|---------|
| `PORT` | autodetect one ttyUSB*/ttyACM* | board UART0 serial port |
| `METHOD` | `probe-rs` | flash path used by `hil/flash.sh`; set `hisiflash` for the vendor path |
| `PROBE_RS_YAML` | none | WS63 chip-description YAML for the probe-rs path |
| `LOADERBOOT` | autodiscover from the SDK | vendor loaderboot.bin, only for `METHOD=hisiflash` |
| `ADDRESS` | ws63 `0x230000`, BS2X `0x90000` | program flash offset for `METHOD=hisiflash` |
| `BAUD` | hisiflash 921600 | flash baud |
| `UART_BAUD` | `115200` | the example's UART0 baud |
| `SETTLE` | `4` | seconds to read UART after each flash |
| `MONITOR` | raw `cat $PORT` | command that prints raw UART (for non-standard adapters) |
| `HISIFLASH` | `hisiflash` | the flash CLI binary |

## Relationship to the other skills

- `qemu-smoke` = software-in-the-loop (boot in QEMU). `hil-smoke` = hardware-in-the-loop
  (flash silicon). Same chip/example surface so results line up.
- `qemu-vs-hil` runs **both** and diffs the markers — the QEMU↔silicon parity check.
- On a HIL failure, hand the captured UART + the example to the **`hil-regression`** skill.

## Gotchas

- **No board ⇒ can't flash.** Use `--preflight`; the real path fails fast with a clear
  message if the port/loaderboot/hisiflash are missing.
- **BS2X HIL is not wired yet** — the QEMU path is solid; on-silicon BS21/BS20 awaits
  a chip-specific board rig, runner label, and flash variables.
- `ADDRESS` is a flash *offset*, not the XIP base — wrong value can misflash. Verify it.
- The actual flash goes through `hil/flash.sh` with `HIL_CONFIRM=1`; the default WS63
  path is patched `probe-rs`, while `METHOD=hisiflash` uses the vendor loaderboot/YMODEM path.
- A first board may need the patched `probe-rs`, `hisi-fwpkg`, and `gdb-multiarch`
  (see `docs/src/how-to/07-run-hil-tests.md` and `docs/src/reference/08-cli-tools.md`).
