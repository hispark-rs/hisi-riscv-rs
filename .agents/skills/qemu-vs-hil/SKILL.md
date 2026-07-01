---
name: qemu-vs-hil
description: Run the same ws63-rs example through both QEMU (qemu-smoke) and real silicon (hil-smoke) and diff the UART markers — the QEMU↔silicon parity check. Use to prove the emulator matches hardware, especially the timing-sensitive checks QEMU can't validate (160 MHz UART baud, 24 MHz TCXO timer period). With no board it fills the QEMU column only.
disable-model-invocation: true
---

The HIL layer's reason to exist, as one command: boot an example in QEMU **and** flash it
to a board, then compare the two UART outputs against one shared marker table. The QEMU
model's credibility rests on this comparison — and the timing-sensitive rows (baud,
timer period) are exactly what emulation cannot prove (see the bring-up steps in
`hil/README.md`). User-invoked: the HIL side writes to hardware.

## Usage

```bash
bash .agents/skills/qemu-vs-hil/parity.sh <chip> [example]
#   chip:    ws63 | bs21 | bs21e | bs22 | bs20
#   example: omit → the common UART-marker set (uart_hello, timer_irq, gpio_irq,
#                                               reset_demo, spi_loopback, i2c_scan)
```

Output is a parity table:

```
  example        QEMU     HIL      match   note
  -------        ----     ---      -----   ----
  uart_hello     PASS     PASS     ✓       ← 160 MHz baud base
  timer_irq      PASS     PASS     ✓       ← 24 MHz TCXO period
  gpio_irq       PASS     PASS     ✓
  …
  PARITY: QEMU ≡ silicon on all markers ✓
```

## What it does

1. Builds the example once (chip-aware: WS63 workspace vs BS2X isolated workspace).
2. **QEMU side** — boots `-M <chip>` and greps the shared marker.
3. **HIL side** — runs `hil-smoke --preflight`; if the rig is ready, flashes + reads UART
   and greps the *same* marker. If not (no board), the HIL column reads `n/a` and only the
   QEMU column is filled.
4. Flags `DIVERGE` when QEMU and silicon disagree on a marker, and points you at the
   `hil-triage` subagent for the failing log.

## Why the timing notes matter

These two rows are the headline parity checks — a green QEMU here means nothing until
silicon agrees:

| row | divergence means |
|-----|------------------|
| `uart_hello` ← 160 MHz baud base | banner garbled / silent on silicon ⇒ the UART clock-divider assumption is wrong |
| `timer_irq` ← 24 MHz TCXO period | period off by ~10× ⇒ the timer is still being computed at 240 MHz, not the 24 MHz TCXO |

## Composition

Thin orchestrator over the two engines — it does not duplicate their logic:
`qemu-smoke` (SIL) + `hil-smoke` (HIL), one shared marker table. On a divergence,
hand the captured UART to the **`hil-triage`** subagent.

## Gotchas

- **No board ⇒ QEMU-only.** That is still useful (confirms the emulator side); the table
  says `qemu-only` per row and `HIL unavailable` at the end.
- Builds and boots are `timeout`-bounded; `spi_loopback` needs MOSI↔MISO shorted on the
  real board or its HIL row will (correctly) diverge.
- `TIMEOUT`, `QEMU_BIN`, `WS63_QEMU`, and all `hil-smoke` env vars (`PORT`/`LOADERBOOT`/…)
  pass through.
