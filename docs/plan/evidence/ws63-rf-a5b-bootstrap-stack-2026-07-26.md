# WS63 RF A5B Bootstrap Stack Evidence

## Scope

This evidence closes a blocking bootstrap failure without changing the default
blocking backend or claiming that vendor initialization is incremental. The
test required no access point and used no network credentials.

Pinned release inputs:

- `hisi-riscv-rt 0.5.6`, revision
  `f283aa0469f3f7e016705f02379c5b6c832cc1c5`;
- `hisi-rf-ws63 0.1.0-alpha.20`, revision
  `13b1bfe8609c0686e83fda7aa875c86d756e8933`.

## Root Cause

The ordinary 8 KiB main stack overflowed during the synchronous
`uapi_wifi_init` call. The overflow corrupted adjacent vendor `.bss`; the
observed `g_event_task` values included `0x30303030`.

The investigation ruled out the PAC register model, RTOS queue and handle
semantics, RF archive relocation, ROM call fallback, and flash programming as
the cause of this bootstrap failure.

## Fix And Layout

`hisi-riscv-rt 0.5.6` adds the explicit
`ws63-radio-main-stack-32k` linker profile. The RF bootstrap fixture selects
that profile and reports `main_stack_bytes_required = 32768`. Ordinary WS63
firmware keeps the existing 8 KiB default.

The verified RF ELF had:

```text
__stack_size    = 0x00008000
__stack_start__ = 0x00a2fc20
__stack_top__   = 0x00a37c20
__heap_start__  = 0x00a39020
```

The independently rebuilt `uart_hello` ELF still reported
`__stack_size = 0x00002000`.

The RF link contract also fails closed on ROM fallback and callback veneer
layout. `hisi-rf-ws63` now emits both `-Thisi-riscv-link.x` and `--no-relax`;
the latter preserves the fixed, link-asserted ROM callback veneer order.

## Real-Silicon Evidence

A full program and readback verification at a 3 MHz probe clock completed in
75.23 seconds. Hardware reset then produced:

```text
RFDBG_BOOTSTRAP_PROFILE_OK
```

The same image was exercised through 20 hardware resets without reflashing:

- bootstrap success: 20/20;
- errors: 0;
- `wifi_init completed`: 20/20;
- observed `vendor_wifi_initialize` time: 61 ms in 3 runs and 62 ms in
  17 runs.

Transient bytes before the first UART marker were capture-alignment noise; all
required markers were complete. The flashboot message
`Flash Init Fail! ret = 0x80001341` remained the known expected board behavior.

## CI And Release Evidence

- RT CI run `30194568618`: passed.
- RT publish run `30194608951`: passed; crates.io independently resolved
  `hisi-riscv-rt 0.5.6`.
- RF CI run `30195327948`: passed, including Ubuntu, macOS, and Windows final
  links for both the plain firmware and bootstrap profile.
- RF publish run `30195406751`: passed; crates.io independently resolved
  `hisi-rf-ws63 0.1.0-alpha.20`.

## Evidence Boundary

This result establishes a measured, repeatable bootstrap baseline and removes
the stack-corruption blocker. It does not make `uapi_wifi_init` interruptible,
pollable, or budget-bounded. Scan, connect, disconnect, supplicant poll, wake
counts, and queue high-water values still require a separate on-silicon
baseline. Full caller-owned task-stack and supplicant-arena admission also
remains open.

The pure-WPA3 HIL gate is unchanged and remains externally blocked.
