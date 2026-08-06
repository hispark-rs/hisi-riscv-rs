# WS63 BLE B0 Archive And ABI Evidence (2026-08-06)

## Scope

This evidence closes B0: the exact WS63 BLE archive set, target ABI, vendor
relocation inventory, external-symbol ownership, normalized Cargo artifacts,
and cross-platform consumer contract are fixed before controller/host startup is
implemented. It does not prove BLE initialization, advertising, scanning, GATT,
pairing, bonding persistence, coexistence, or silicon behavior.

The release input is `ws63-radio-sys` commit `128e996`, tagged
`v0.1.0-alpha.12`. The two implementation commits are:

- `abb794c`: hash-bound archive/ABI inventory and generated report;
- `b4949dc`: normalized redistributable archives and Cargo consumer contract.

## Frozen Archive Set

| Archive | Source SHA-256 | Members | Vendor relocations | Role |
|---|---|---:|---:|---|
| `libbt_host.a` | `5e74634a15bf348436de991fa9585bb198ae67ad7a22ed659af7586c8a6e62f3` | 87 | 3,079 | ATT/GATT/GAP/L2CAP/SMP host |
| `libbt_app.a` | `6fe84ddea4e02a444afe6d7b1ef1e63cd15e8d2b8d8ad648ea2525062308c9fc` | 28 | 1,279 | application service and connection adapter |
| `libbth_sdk.a` | `dc3aef21864a97c6c053cb67654ea363e8e83aed4d07fcb2a60cd05054670378` | 9 | 382 | public BLE SDK entry points |
| `libbg_common.a` | `40c671a542c3d1a9be413786f1d75b7dcd9635d7985b507c64efa7b3ff713dcd` | 44 | 618 | shared transport/runtime/crypto support |

`libbth_gle.a` is explicitly excluded because it is the SLE/GLE host stack. Its
hash and 3,542 vendor relocations remain recorded so it cannot enter the BLE-only
profile silently.

All selected inputs are ELF32, little-endian RISC-V objects with `e_flags=0x3`
(RVC and single-float ABI). Across the four archives the report records 2,573
defined global symbols, 1,435 undefined global symbols, and 120 required external
symbols after archive closure.

## Ownership And Relocation Closure

Every required external symbol has an explicit integration owner: 42 RTOS, 39
ROM, 14 diagnostics, 9 compiler/core libc, 5 allocator, 3 controller transport,
3 application hooks, 2 NVS, 2 explicit SLE boundary, and 1 crypto symbol. An
unknown owner fails report generation and CI.

The input contains 5,358 vendor relocations: 1,404 `R_RISCV_48_LLUI`, 1,298
`R_RISCV_BRANCHI`, and 2,656 `R_RISCV_LLUI_REP`. Every BRANCHI is same-section;
cross-section count is zero. Release normalization converts or resolves the
declared relocation classes and fails on an unknown vendor relocation. Rebuilding
all release artifacts reproduced every committed archive byte-for-byte.

The normalized BLE output hashes are:

- `libbt_host.a`: `a76fc6958aeaeb3ac8fb695b8bde02ba954c8fefef01e8bd345a70fa0ffdb702`;
- `libbt_app.a`: `7f52c92bdf1b5fc58a486d8a34895104f9cd3c0fd82f3ec90b02be2005b346de`;
- `libbth_sdk.a`: `740195f7fcb885b8602fc7b891f756a290a0cd73a09b922950217b213cf1e1dd`.

## Build And Release Evidence

Local preflight passed:

- 18 host tests across library, CLI, and integration suites;
- workspace Clippy with `-D warnings`;
- RV32 `ws63-radio-sys --no-default-features --features ble` build;
- release artifact reproducibility and package checks;
- `ws63-radio-blob` package size 7.9 MiB compressed, below the registry limit.

GitHub Actions run
[31055612394](https://github.com/hispark-rs/ws63-radio-sys/actions/runs/31055612394)
passed the full test/rebuild lane plus submodule-free Linux x86_64, macOS arm64,
and Windows x86_64 Cargo consumer builds. Publish run
[31055799204](https://github.com/hispark-rs/ws63-radio-sys/actions/runs/31055799204)
rebuilt the pinned native archives, reran release gates, and published
`hisi-rf-link`, `ws63-radio-blob`, and `ws63-radio-sys` `0.1.0-alpha.12` to
crates.io. Each crate was then downloaded from the registry outside the source
workspace.

## Evidence Boundary

B0 proves that the BLE integration starts from a reviewable, reproducible, and
fail-closed archive contract. The `ble` feature exports packaged archive paths,
the profile revision, and the generated report; it intentionally does not link a
startup closure or expose a user-facing BLE API. B1 must now prove controller/host
initialization, transport, NVS identity/bonding reads, allocator/RTOS resource
admission, and a real-silicon marker before B2 begins.
