# WS63 Radio U5C Bond Removal And NVS GC Evidence (2026-08-20)

## Scope

This evidence closes vendor-managed BLE bond removal across `nRST`, including
the full-page NVS compaction path. It does not transfer ownership of the opaque
vendor SMP record to Rust and does not graduate the U5 API to stable.

## Release Closure

- `hisi-storage 0.1.0-alpha.3` adds an explicit erase capability.
- `hisi-nvs 0.1.0-alpha.3` adds deterministic, transactional full-page
  compaction.
- `hisi-hal 0.7.0-alpha.9` adds bounded WS63 SFC sector erase.
- `hisi-crypto-ws63 0.1.0-alpha.5` aligns its HAL dependency without changing
  the cryptographic algorithms.
- `hisi-rf-ws63 0.1.0-alpha.80` implements runtime NV erase/write and uses SFC
  command reads while a writer transaction is active.
- `hisi-rf 0.1.0-alpha.94` makes the next reset, rather than an immediate
  same-boot query, the persistence proof for bond removal.

The compactor copies live records first, erases the destination sector, writes
the canonical records, and writes the page header last as the commit point.
The WS63 read path deliberately avoids XIP during this transaction: command-mode
writes otherwise left prefetched XIP bytes stale and made a successful flash
update appear unchanged to the same boot.

## Artifacts

| Role | SHA-256 |
|---|---|
| Peripheral | `5323f7363310bae59f2f0cede8ba20ad2b6925ca563a71c9b40b5466be1759ab` |
| Central | `580597f51d6533615fb3827952d24f16fa29067e57578eb8f31e17384cb9522c` |

Both images were downloaded with the WS63 probe profile at 3 MHz and full
readback verification. The peripheral's pre-test NVS snapshot was retained at
`/private/tmp/ws63-u5-peripheral-nv-20260820.bin`, SHA-256
`00b284c13d992d7f285c610b6ffec1233f3e7177a6e279d0a84896d98d9b062c`.
This path is a local execution record, not a durable release artifact.

## Verification

Host and build evidence passed:

- `hisi-storage`: 2 tests;
- `hisi-nvs`: 17 tests, including full-page compaction and failure ordering;
- `hisi-hal`: 319 WS63 host tests;
- `hisi-rf`: 10 library, 3 U5 fixture, and 2 contract tests;
- bond reset matrix classifier: 18 tests;
- `hisi-rf-ws63` RV32 check with `ble-init`;
- standalone locked package checks for every released crate.

The corrected removal contract first passed `3/3` at
`/private/tmp/ws63-u5-gc-readfix-3reset-v2-20260820`, then passed `20/20` with
the same images at `/private/tmp/ws63-u5-gc-readfix-20reset-20260820`.
`summary.json` reports schema 4, `expect_removal=true`, `contract_pass=true`,
`persistence.proven=true`, and zero failed runs.

The 20 runs alternated between:

1. `BOND_EMPTY`, followed by a fresh pair and bond;
2. `BOND_RESTORED`, followed by remove, `NotPaired`, and successful NV commit.

Every following reset observed the opposite expected startup state. Restored
runs required both `BOND_REMOVED` and `BOND_REMOVE_OK`; any `RFDBG_NV_WRITE_ERR`
was a hard failure. Full-page runs also emitted `RFDBG_NV_GC_ERASE_BEGIN` and
`RFDBG_NV_GC_ERASE_DONE`.

## Proof Boundary

This matrix proves the fixed artifacts repeatedly complete the vendor-managed
pair, restore, remove, and reset lifecycle on the two attached WS63 boards. Host
failure-injection tests cover the page-header commit ordering. It does not prove
all possible flash power-loss timings, general flash endurance, Rust-managed SMP
record ownership, or pairing responder/cancellation API stability.
