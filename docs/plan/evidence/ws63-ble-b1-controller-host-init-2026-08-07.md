# WS63 BLE B1 Controller/Host Init Evidence

## Scope

This evidence closes B1 only: fixed archive closure, caller-owned resources,
native RTOS task/queue/timer/IRQ services, controller/host initialization, and a
repeatable target marker. It does not claim advertising, scanning, pairing,
GATT, or a stable user-facing BLE API.

## Immutable Inputs

- `hisi-rf-ws63`: `6cb2faf90d176b3a1a401fef01ce6e4d9b60abda`
- `ws63-radio-sys`: `4fa4e3f29f6b28de4484630f7920c72bdcbd2032`
- `hisi-riscv-rt`: `760fe7d47061ad9c5ff198bd8eaad1293e05ef52`
- `hisi-rf` facade dependency alignment:
  `a5ada7d9659df256fb15c728b401fb166fcc1107`
- default B1 ELF SHA-256:
  `7743ffe36a877b499fa99540607d1f2eb8c74733a4667b86e0318d91ad032042`
- planned flash image SHA-256:
  `8b1da72ee3d371bdf7f28f97310fbcecef64c93ff3a76e2d18d2c7a71b44f467`

The target used the pinned official nightly and `-Zbuild-std=core,alloc`. The
firmware was built with `ble-init,firmware-example`; `ble-init-diag` was not
enabled.

## Software Gates

- `cargo fmt --all -- --check`
- 32/32 host library tests with the diagnostic feature enabled
- RV32 `cargo check` for `ble-init`
- release builds for both default and opt-in diagnostic B1 examples
- RV32 library clippy with `-D warnings`
- `ws63-radio-sys` archive profile v28, callback roots, normalized relocation
  verification, host tests, and package preflight

## Silicon Result

The default image was downloaded with probe-rs at 3 MHz with complete verify;
the measured download phase completed in 55.24 seconds. J-Link nRST then booted
the image and produced the ordered markers through:

```text
RFDBG_BLE_B1_LINK_CONTRACT_OK
RFDBG_BLE_B1_ADMISSION_OK
RFDBG_BLE_B1_TASKS_PRIMED
RFDBG_BLE_B1_ENABLE_BEGIN
RFDBG_BLE_B1_INIT_OK
```

Without reflashing, three further J-Link nRST cycles all produced
`RFDBG_BLE_B1_INIT_OK`. Every run created the six expected bounded queues,
entered all four archive-derived tasks, registered the controller/baseband
interrupt paths, and reported no exception or allocation failure.

## Escaped Diagnostic Defect

During bring-up, the opt-in snapshot worker used a 2 KiB stack while its RV32
optimized frame reserved 2624 bytes: the local 17-entry `TaskDiagnostic` array
alone occupied 2176 bytes. The stack underflow overwrote adjacent calibration
BSS and caused a load fault in the vendor radio path. B1 now gives this
maintainer-only worker a dedicated 4 KiB stack, while the production task
profile retains its archive-derived heterogeneous stack sizes. Both diagnostic
and default images subsequently reached the init marker; the repeated reset
gate above used the default image.

## Boundary

This is integration evidence for one fixed WS63 image and board. It proves B1
parity and catches regressions in the linked controller/host init closure. It is
not a formal RTOS proof and does not establish BLE advertising, scanning, radio
interoperability, or packet-loss guarantees. Those belong to B2 and later HIL
gates.
