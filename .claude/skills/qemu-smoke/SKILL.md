---
name: qemu-smoke
description: Build a ws63-rs example for a HiSilicon chip (ws63/bs21/bs21e/bs22/bs20) and boot it in the hisi-riscv-qemu fork, asserting the expected UART banner / GPIO toggle / embassy interleave / IRQ delivery. Use to validate firmware end-to-end in emulation, or after touching a driver/example/the QEMU model.
disable-model-invocation: true
---

End-to-end emulation check: builds example firmware with the custom `hisi-riscv`
toolchain, then boots it on the matching `-M <chip>` machine in the
[hisi-riscv-qemu](https://github.com/hispark-rs/hisi-riscv-qemu) fork and asserts the
expected output. This is what `run-ws63-rs` (build/lint/test) stops short of — actually
*running* the firmware. User-invoked because it spawns QEMU processes.

## Usage

```bash
bash .claude/skills/qemu-smoke/smoke.sh <chip> [example]
#   chip:    ws63 | bs21 | bs21e | bs22 | bs20
#   example: blinky | uart_hello | timer_irq | gpio_irq | embassy_multitask | …
```

- **Full suite** (omit `example`): builds the chip's example set and delegates the
  assertions to ws63-qemu's per-chip smoke script (`scripts/smoke-test.sh` for ws63,
  `scripts/<chip>-smoke-test.sh` for BS2X) — the source of truth.
  ```bash
  bash .claude/skills/qemu-smoke/smoke.sh ws63      # full WS63 suite
  bash .claude/skills/qemu-smoke/smoke.sh bs21      # BS21 milestone-1 suite
  ```
- **Single example**: builds just that crate, boots `-M <chip>`, applies one focused
  assertion (banner / GPIO trace / `[fast]`+`[slow]` interleave / IRQ marker).
  ```bash
  bash .claude/skills/qemu-smoke/smoke.sh ws63 uart_hello
  bash .claude/skills/qemu-smoke/smoke.sh bs21 blinky
  bash .claude/skills/qemu-smoke/smoke.sh ws63 embassy_multitask
  ```

## What it knows (so you don't have to)

- **Build split** — WS63 examples are root-workspace members (`cargo build -p <ex>`);
  BS2X examples are **isolated workspaces** (`--manifest-path examples/bs21|bs20/Cargo.toml`)
  because one `cargo build --workspace` would unify hisi-riscv-hal features and pull in
  both chips at once (a `compile_error!`). The script builds each the right way.
- **Chip → machine / binaries** — `bs21e` and `bs22` reuse the `examples/bs21` (chip-bs21)
  binaries booted under their own `-M`; `bs20` has its own dir (128K `memory.x`). BS2X
  binaries carry a chip prefix (`bs21_uart_hello`); WS63 ones don't (`uart_hello`).
- **QEMU discovery** — uses `qemu-system-riscv32` on `PATH`, else `$WS63_QEMU/qemu/build/…`
  (autodetects `/root/ws63-qemu`), and **builds the fork** via its `scripts/build.sh` if the
  binary is absent. Verifies the `-M <chip>` machine exists before running.
- **Runner** — `-M <chip> -nographic -bios none -kernel <elf>`; for `blinky` adds
  `--trace 'ws63_gpio_*'` to observe real GPIO set/clear; each boot is `timeout`-bounded.

## Assertions

| example | passes when |
|---------|-------------|
| `uart_hello` | UART banner naming the chip (`Hello … <chip> … on QEMU` / `UART0 … alive`) |
| `blinky` | >1 `ws63_gpio_*` toggle events, no illegal/fault |
| `async_*` / `embassy_*` | both `[fast]` and `[slow]` task lines appear (embassy executor) |
| `timer_irq` | `timer interrupts delivered` (TIMER_0, IRQ 26) |
| `gpio_irq` | `local IRQ (>=32) delivered` (custom LOCI interrupt) |
| other | no fault/panic; prints first UART lines for inspection |

## Env overrides

| var | default | purpose |
|-----|---------|---------|
| `WS63_QEMU` | autodetect `/root/ws63-qemu` | the QEMU fork checkout |
| `QEMU_BIN` | PATH → `$WS63_QEMU/qemu/build/qemu-system-riscv32` | emulator binary (built if missing) |
| `TIMEOUT` | `5` | seconds per boot before kill |
| `PROFILE` | `release` | `release` or `debug` |

## Gotchas

- Needs the **`hisi-riscv` toolchain** (see the `run-ws63-rs` skill) — firmware won't
  build otherwise.
- Boots loop forever; `timeout` kills them — a non-zero exit from `timeout` (124) is
  expected and not a failure by itself. The assertion is on the captured output.
- First run may **build the QEMU fork** (several minutes). Subsequent runs reuse it.
- UART0 is on stdio via `-nographic`; the script feeds `</dev/null` so the firmware's
  read side never blocks.
