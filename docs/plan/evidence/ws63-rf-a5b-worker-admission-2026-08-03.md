# WS63 A5B Worker Admission Evidence

## Scope

This evidence closes the r8 incremental-worker bootstrap admission regression
on real WS63 silicon. It covers the split task reservation, RF initialization,
one credential-free scan, and the bounded native runner marker.

It does not cover association, WPA parity, DHCP, ping, cancellation, late
completion, or the externally blocked pure-WPA3 gate. The fixture credentials
were public non-matching placeholders; no user network credential was read or
recorded.

## Root Cause

The r8 profile correctly reserves two owner-bound task groups:

- seven vendor tasks with 24 KiB stacks;
- one Rust incremental worker with an 8 KiB stack.

After the seven vendor slots were reserved, the vendor bootstrap still checked
the complete eight-task profile against the remaining capacity. Its diagnostic
`0x21000807` decodes to `required=8, available=7`. This double-counted the Rust
worker even though its reservation is owned and consumed separately.

`hisi-rf-ws63` commit `8c5e1aa` introduced an explicit seven-slot vendor
contract while retaining eight as the complete incremental profile total. The
fix was released as `hisi-rf-ws63 0.1.0-alpha.64` and consumed through
`hisi-rf 0.1.0-alpha.74`.

## Automated Verification

- 106 host tests passed with the exact WPA3 incremental worker feature set.
- Strict clippy passed for the WPA3 incremental backend.
- The WPA3 incremental connect profiler linked for
  `riscv32imfc-unknown-none-elf` with stock rust-lld.
- The final ELF retained 37 ROM patch entries and zero vendor relocations.
- `hisi-rf-ws63` main CI run `30772592405` passed.
- `hisi-rf-ws63` publish run `30772751545` passed.
- `hisi-rf` publish run `30772816014` passed.

## Silicon Result

The image was downloaded through the FlashPlan binary path at 3 MHz with full
write verification, then reset through the explicitly paired J-Link while
UART0 was already open. Download and verification took 98.95 seconds.

The same boot emitted these stage markers:

```text
RF1_IMAGE_OK
RF2_INIT_BEGIN
RF2_INIT_OK ifname=hisi-rf
A4_RADIO_EVENT kind=initialized
RF3_SCAN_BEGIN
RF3_SCAN_OK count=0x00000004 truncated=0x00000001
A4_RADIO_EVENT kind=scan-completed
W2D_NATIVE_RUNNER_RX_READY
W2E_AP_NOT_FOUND ssid=<redacted>
```

The HIL contract summary was `{"pass": 1}`. The expected final AP-not-found
marker proves that initialization and scanning completed before the public
fixture failed to match a real network; it is not connectivity evidence.

## Remaining Gate

A5B remains opt-in. Its worker preemption, cancellation, late completion, and
full connectivity parity still require repeated dual-board HIL before the
incremental path can replace the current default.
