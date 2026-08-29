# WS63 Radio U5D Negative Pairing Evidence (2026-08-29)

## Scope

This evidence closes the two U5D negative pairing lifecycle gates on two WS63
boards: explicit user rejection and stale connection-generation rejection. It
does not graduate the BLE API to stable, prove interoperability with arbitrary
peers, or replace the separate positive Secure Connections and bond persistence
evidence.

## Evidence Integrity Fix

An earlier reject matrix reported `19/20`, but its reset orchestration restarted
the peripheral and waited three seconds before resetting the central. During
that interval the preceding central generation could connect to the new
peripheral. Its UART was not drained until later, so previous-generation reject
markers could satisfy the next record. That entire matrix, including its apparent
passes, is invalid evidence.

Parent commit `160a7a2fe` closes this gap. The matrix now holds central nRST while
the peripheral restarts and begins advertising, then releases central nRST. The
reset owner releases the pin on normal and exceptional exits. Schema 7 also
requires exactly one `boot.` marker per board and rejects any U5 protocol marker
before that boot marker. Host tests cover exceptional reset release, prior-
generation markers, and multiple boot generations.

## Artifacts

| Mode | Role | SHA-256 |
|---|---|---|
| Reject | Peripheral | `337eada656563d1ba3d3f783a79640e357f8c3310bcc87e5e83a862df632b749` |
| Reject | Central | `5aa81903f2f7fe1aca73b32c0edaf28463538e92b8a6577674979f45b8757c83` |
| Stale generation | Peripheral | `44ae8b184aa9b7843c782c0cbaf969343a8a40f59f59618a4bae30c1b929c698` |
| Stale generation | Central | `2e1c53e9fb03f2be5ceebe87b1ec3568ddbddba50f56e78b6061d7d10261f1a4` |

The stale images were downloaded at 3 MHz through the `hisi-fwpkg` FlashPlan
image and probe-rs binary path with full readback verification. Peripheral took
61.86 seconds and central 83.50 seconds. Local artifact directories are execution
records, not durable release artifacts.

## Verification

The schema-7 reject fixture passed:

- `3/3`: `/private/tmp/ws63-u5d-negative-reject-generation-isolation-3reset-20260829`;
- `20/20`: `/private/tmp/ws63-u5d-negative-reject-generation-isolation-20reset-20260829`.

Every run started both roles from `BOND_EMPTY`, observed explicit rejection and
negative disconnect completion, and ended at the role-specific `REJECT_OK`
marker. The 20-run summary has `contract_pass=true`,
`persistence.proven=true`, zero restored runs on both boards, and no generation
failure.

The unchanged stale-generation fixture passed:

- `3/3`: `/private/tmp/ws63-u5d-negative-stale-generation-isolation-3reset-20260829`;
- `20/20`: `/private/tmp/ws63-u5d-negative-stale-generation-isolation-20reset-20260829`.

Every run invalidated the current connection generation, rejected the old
responder with `StaleLifecycle`, completed negative disconnect, and reached the
role-specific `STALE_OK` marker. Its 20-run summary also has
`contract_pass=true`, `persistence.proven=true`, zero restored runs, and no
generation failure.

Host verification passed 5 reset-helper tests, 28 matrix tests, the parent uv
entry-point contract, and `git diff --check` before the silicon matrices.

## Proof Boundary

These matrices are integration and statistical evidence for the exact fixtures,
boards, runner commit, and environment above. They prove that the observed
negative lifecycles neither authenticate nor persist a bond across the tested
resets, and that each record belongs to one firmware generation. They do not
prove that RF traffic, external peers, all power-loss timings, or future archive
versions can never fail. Positive pairing, restoration, and removal remain bound
to their own evidence records.
