# WS63 Radio U3 Static Database Evidence

## Status

U3 implementation and the BLE paired-board gate are complete. The SLE paired-board gate is
pending because the execution environment rejected the required USB/J-Link escalation after its
approval quota was exhausted. This is not a firmware, probe, build, or board failure. U3 remains
open until the fixed SLE images pass both the 3-reset shape gate and 20-reset matrix.

## Immutable Inputs

- `hisi-rf-core`: `e6e52ca1979108a992be2961268d8595992ce5f4`
- `hisi-rf-ws63`: `f0626ac33b9062380da7ac7465fe2760cde5bf2d`
- `hisi-rf`: `f2fad7247a682e22ea0d4d4e897f1b263b16d04f`
- `ws63-radio-sys`: `c85ffd0965a7bbbb8f4b979500a6593181699176`
- parent before the U3 harness/evidence pointer commit: `ef4c7acf27efbae23bbd508d4a19940e7f7de18d`

The four fixed ELF artifacts are:

| Role | SHA-256 |
|---|---|
| BLE typed GATT server | `49ae5ce50254fe49f12490e79de5d5760c33a5ebf9c1ec62652e8905008ad48a` |
| BLE U2 scanner oracle | `eccc424d48db2fe5e08d8251c014dd33e706c14720b5195a77767bb06840915f` |
| SLE typed SSAP server | `a36fcfabc80692aa19940eb7d8b56983b63a1c47c0fb3c2e7d0cbf6b96eb11c9` |
| SLE U2 seeker oracle | `05d25cd9680afcb778f288af7dda902e88cea400ab76fb332d2d4c1445c32050` |

## Contract

- `hisi-rf-core` owns chip-neutral UUID, permission, property/operation and static database types.
- The WS63 adapter accepts only the reviewed one-service, one-characteristic/property and
  one-descriptor profile. Larger valid generic databases fail closed with `UnsupportedDatabase`.
- Initial values are copied into backend-owned fixed storage before the vendor registration calls.
- `hisi-rf` commands carry typed definitions and return opaque `GattServerHandle` or
  `SsapServerHandle`; backend IDs and `hisi-rf-ws63` types do not appear in the public API.
- The existing B3/S3 backend matrices remain the complete protocol behavior oracle. U3 tests the
  new facade/schema path and does not claim U4 event, cancellation or stale-handle semantics.

## Verification

- `hisi-rf-core`: 28 host tests, clippy and public API baseline pass.
- `hisi-rf-ws63`: BLE 37 host tests and SLE 35 host tests pass; both profiles pass host clippy and
  RV32 `cargo check -Zbuild-std=core,alloc`.
- `hisi-rf`: both profile host suites pass 2/2, both profiles pass host clippy and RV32 checks, and
  the U3 public API snapshots pass the implementation-leak scanner.
- Both U3 source examples link in release mode with stock rust-lld and no stage API in application
  code.
- BLE source and observer were downloaded through the FlashPlan binary path at 3 MHz with complete
  probe-rs verification, approximately 50 seconds per image.
- BLE passed `/private/tmp/ws63-radio-u3-20260808-ble-3reset` at 3/3 and
  `/private/tmp/ws63-radio-u3-20260808-ble-20reset` at 20/20. Every round contains
  `RFDBG_RADIO_U3_BLE_GATT_REGISTERED`, `RFDBG_RADIO_U3_BLE_GATT_OK`, and observer
  `RFDBG_RADIO_U2_BLE_SCAN_OK`; no failure marker, panic, missing ROM callback, or event drop was
  observed.

## Remaining Gate

Flash the two fixed SLE artifacts through the same FlashPlan binary path at 3 MHz with complete
verify, then run protocol `u3-sle` through the paired reset harness for 3 and 20 resets. Required
markers are source `RFDBG_RADIO_U3_SLE_SSAP_OK` and observer
`RFDBG_RADIO_U2_SLE_SEEK_OK`. Only a 3/3 plus 20/20 result closes U3 and permits the plan to move to
U4.
