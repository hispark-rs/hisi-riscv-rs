# WS63 Radio U2 Typed Control Evidence

## Scope

This evidence closes U2 for the application-facing `hisi-rf` BLE GAP and SLE
announce/seek slices. It proves that facade-owned typed commands cross the
bounded controller/runner boundary and reach the existing WS63 backend on two
boards. It does not claim typed GATT/SSAP databases, public unsolicited-event
streams, cancellation, pairing, coexistence, or stable API completion.

## Immutable Inputs

- `hisi-rf-core`: `7509b11`
- `hisi-rf-ws63`: `86bcfe0`
- `hisi-rf`: `91c5cf8`
- parent paired-matrix harness: `081ceece4`
- `ws63-radio-sys`: `0.1.0-alpha.17` (`c85ffd0`)
- `hisi-rf-rtos-driver`: `0.1.0-alpha.19`
- `hisi-rtos`: `0.1.0-alpha.24`, with the WS63 port enabled

The four fixed ELF artifacts were:

| Role | SHA-256 |
|---|---|
| BLE advertiser | `7315b61914db100de1051998dfd42422d7a162cc9e09486802e39c79583e2c79` |
| BLE scanner | `5cb461206511849f14b36d566287cccda6d95f6d61bdd9a58103ed96f98b7dc5` |
| SLE announcer | `286790efea73c3266b1182e9512be096d8f2c439ef1870f988f374a10b03e428` |
| SLE seeker | `63dc85e49c77de9b5a1a34b9c4406ff4bb1ffab04476dc9be066a83d0cf9c4aa` |

## Contract

- Applications construct only facade-owned storage/resources, initialize one
  `RadioController`, call `split()`, and retain the mandatory `RadioRunner`.
- BLE and SLE profiles remain compile-time exclusive until coexistence has its
  own evidence.
- Typed BLE advertise/scan and SLE announce/seek commands use one bounded
  command owner and do not expose stage, sys, blob, or RTOS-driver types.
- The U2 firmware yields through the RF runtime sleep contract. Merely pending
  a software interrupt is not an explicit yield from a Cooperative main task.
- HIL-only lifecycle observations remain hidden behind
  `u2-hil-diagnostics`; they are not public protocol events.

## Verification

- Both profile-specific host contract suites pass: 2 tests for BLE and 2 for
  SLE on `aarch64-apple-darwin`.
- Both RV32 profile libraries and all four U2 examples pass targeted clippy
  with `-D warnings`; `cargo fmt --all -- --check` passes.
- All four ELF images were downloaded at 3 MHz through the FlashPlan binary
  path with complete probe-rs verification. Measured downloads were about
  50.6 seconds for each BLE image and 66.4 seconds for each SLE image.
- BLE passed a 3/3 shape gate and a fixed-image 20/20 paired nRST matrix. Every
  round reached `RFDBG_RADIO_U2_BLE_ADV_OK` on the source and
  `RFDBG_RADIO_U2_BLE_SCAN_OK` on the observer.
- SLE passed a 3/3 shape gate and a fixed-image 20/20 paired nRST matrix. Every
  round reached `RFDBG_RADIO_U2_SLE_ANNOUNCE_OK` on the source and
  `RFDBG_RADIO_U2_SLE_SEEK_OK` on the observer.
- Both 20-reset summaries report zero failed rounds, missing required markers,
  command errors, panic markers, and event drops.

The Linux CI host command uses a native `x86_64-unknown-linux-gnu` runner. A
macOS attempt to cross-link that test target was rejected by the host linker,
so local evidence uses the native Apple host target and leaves the Linux row to
CI rather than weakening linker flags.

## Boundary

U2 proves facade ownership and typed control for BLE GAP discovery and SLE
announce/seek only. Existing B3 GATT and S3 SSAP backend behavior remains the
oracle for U3, but those stage APIs are not promoted by this evidence. U3 must
introduce static typed databases/handles without leaking backend identifiers;
U4 must separately close unsolicited events, cancellation, stale completion,
and lifecycle handles before any stable graduation review.
