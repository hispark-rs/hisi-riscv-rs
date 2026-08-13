# WS63 Radio U5C Vendor Bond Restore Evidence (2026-08-13)

## Scope

This evidence closes vendor-managed BLE bond persistence and restore across
`nRST`. It does not close removal persistence, Rust-managed key ownership, or
stable API graduation.

## Release Closure

- `ws63-radio-sys 0.1.0-alpha.19` retains the five indirect SMP callback
  providers required by the fixed BLE archives.
- `hisi-rf-ws63 0.1.0-alpha.77` verifies those providers against the exported
  archive metadata.
- `hisi-rf 0.1.0-alpha.90`, commit `9a08fe3`, restarts the security procedure
  after reconnect and verifies the restored peer with
  `gap_ble_get_pair_state == Paired` after `PairingComplete`.
- GitHub Actions run `31698721473` passed the full facade CI matrix. Publish run
  `31699129038` published `hisi-rf 0.1.0-alpha.90`.

The restored path deliberately does not require a successful
`AuthenticationComplete` callback. The vendor BLE samples treat successful
`PairingComplete` as the usable security completion, while the authentication
callback is not guaranteed when stored keys are reused. The central therefore
queries the typed pairing state after that completion; the peripheral requires
a current-connection security completion before reporting success.

## Artifacts

Both artifacts were rebuilt from tag `v0.1.0-alpha.90` with the pinned official
nightly and `-Zbuild-std=core,alloc`:

| Role | SHA-256 |
|---|---|
| Peripheral | `78ee362bac2c92e2ebf5c80fc07b54c9c4ab5a3d36938a740e89adecd92552b3` |
| Central | `5f8850c93c98e276d7f87c05d9da68a708ad441e87fe2e5ccd7d78e0fb6ec838` |

The same hashes were already present on the boards after 3 MHz downloads with
full verify, so the final matrix did not rewrite flash again.

## HIL Result

The paired-board 3-reset gate passed `3/3`. The release-tag matrix then passed
`20/20`; all 20 runs reported restored startup state on both boards and no
failure marker. `summary.json` reports:

- `contract_pass: true`;
- `persistence.proven: true`;
- peripheral restored runs: `20/20`;
- central restored runs: `20/20`;
- failed runs: `0`.

Each restored run required connection, a current security completion,
`PairingState::Paired`, `RESTORED_ACTIVE`, and `BOND_OK`. The raw artifacts are
under `/private/tmp/ws63-u5-alpha90-tag-20reset-20260813`; this local path is an
execution record, not a durable release location.

## Remaining Gate

U5C is not fully complete until removal is proven across reset:

1. remove the selected bond;
2. query `NotPaired` in the same boot;
3. reset both boards;
4. require `BOND_EMPTY` and a fresh security lifecycle;
5. repeat with event-conservation and stale-handle checks.

This evidence does not claim that Rust owns the opaque vendor SMP record. The
supported mode remains vendor-managed persistence with a bounded Rust observer.
