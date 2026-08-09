# WS63 Radio U4 Lifecycle Evidence

## Scope

This evidence closes the U4 silicon gate for bounded BLE and SLE lifecycle
events, generation-tagged active guards, explicit asynchronous stop, and
best-effort Drop cleanup. It does not claim pairing, bonding, coexistence,
stable public BLE/SLE APIs, or general event delivery under every RF
environment.

Each fixed firmware executes three lifecycles before emitting its pass marker:

1. start followed by explicit `stop(self).await`;
2. start followed by guard Drop and bounded runner progress;
3. restart followed by explicit stop.

The third start is the stale-generation check: cleanup from the dropped guard
must not cancel or corrupt the new operation.

## Immutable Inputs

- `hisi-rf-core`: `fc7435fa86dc1429872c0cac95bc62c0afe0223e`
- `hisi-rf-ws63`: `e04565a90894f64b23860bd7bfb973070341da3c`
- `hisi-rf`: `cdcac5c17714e0cc044e13e80d2f0aed42409b92`
- `ws63-radio-sys`: `c85ffd0965a7bbbb8f4b979500a6593181699176`
- `hisi-rtos`: `a184dd6942430690373cc67f025aaf9332d7b690`
- parent matrix contract: `2d4191e490f6bdc30e23b1d63c917660d14b0981`
- BLE advertiser ELF SHA-256:
  `493be81cb63d7bd7f090945f17f56dd6d78410bda4b70dc588aa25a155ed13d8`
- BLE scanner ELF SHA-256:
  `63ba28aa052fd872901e05e9f99092183244211124a46c4e98f8c7a35d9d78d7`
- SLE announcer ELF SHA-256:
  `f80ff5fa8a29758ee5d947af9d286e822cac734d0afdc7c24138967a1d68e7e7`
- SLE seeker ELF SHA-256:
  `a00ecadac7b1accb407a3f1a557c12777a3a189aa840a64e6e305e6c65a477a5`
- BLE advertiser/scanner FlashPlan image SHA-256:
  `9aed6949a443f078e2e21117e8ca8b84ad984e17d54ee6e295997c30b09aec56` /
  `6513bdf0eddd761372d6349db6071312821677204618404ff6cfa86d7dedb8d6`
- SLE announcer/seeker FlashPlan image SHA-256:
  `c19514f4073732eceafb72ab3e73bd39d700802b96799e8d36e24577d8daf6e4` /
  `460a6bcdf2d8d28f8e1f9340a2b41705ee7a3e77f03846d14d6e4efc294ed5c6`
- build target: `riscv32imfc-unknown-none-elf`, release, `--no-relax`
- download path: `hisi-fwpkg plan` image followed by probe-rs binary download
  at 3 MHz with complete verify

The BLE images completed download and verify in approximately 60 seconds each;
the larger SLE images completed in approximately 68 seconds each. No 3 MHz
program, verify, or DMI failure occurred in these four final downloads.

## Software Gates

- BLE and SLE facade host tests and public API snapshot checks
- BLE and SLE backend host tests and contract tests
- BLE and SLE host clippy with `-D warnings`
- BLE and SLE RV32 checks and clippy for all four U4 fixtures
- parent matrix contract tests: 7/7
- parent Python uv contract: 53 first-party scripts
- `mdbook build docs`

## Diagnostic History

The first BLE shape matrix was 0/3 even though both sides reached
`RFDBG_RADIO_U2_INIT_OK`. The U4 fixtures polled `RadioRunner` while waiting for
start/stop events but did not yield to the RTOS, so vendor tasks could not
advance after initialization. Commit `cdcac5c` adds the same explicit scheduler
handoff used by the established U2 fixtures. Rebuilt fixed images then passed
the BLE 3/3 shape gate. This was a fixture scheduling defect, not accepted as a
backend lifecycle failure.

## Silicon Result

Board A ran the advertiser or announcer image and board B ran the scanner or
seeker image. Each final image was downloaded once with complete verify. The
shape gates and final matrices then used only the two J-Link nRST lines while
capturing both UART streams.

- BLE shape gate: 3/3
- BLE final matrix: 20/20
- SLE shape gate: 3/3
- SLE final matrix: 20/20

Every final BLE run contained:

```text
RFDBG_RADIO_U2_INIT_OK
RFDBG_RADIO_U4_BLE_ADV_LIFECYCLE_OK
RFDBG_RADIO_U4_BLE_SCAN_LIFECYCLE_OK
```

Every final SLE run contained:

```text
RFDBG_RADIO_U2_INIT_OK
RFDBG_RADIO_U4_SLE_ANNOUNCE_LIFECYCLE_OK
RFDBG_RADIO_U4_SLE_SEEK_LIFECYCLE_OK
```

All 40 final records have empty missing-marker and failure-marker sets. BLE
source/observer logs are fixed at 2426/2427 bytes per run; SLE source/observer
logs are fixed at 789/785 bytes per run. The executable evidence contract is
`hil/ws63-radio-u2-reset-matrix.py`. Raw logs and summaries remain under:

- `/private/tmp/ws63-radio-u4-ble-20reset-20260809`
- `/private/tmp/ws63-radio-u4-sle-20reset-20260809`

## Boundary

This is integration and statistical regression evidence for four fixed images,
two boards, and the stated environment. It proves that the tested facade,
backend, RTOS, and vendor stack completed the bounded U4 lifecycle without an
observed command-correlation, event-drop, stale-generation, stop, or panic
failure. It is not a mathematical liveness proof and does not graduate the BLE
or SLE APIs to stable.
