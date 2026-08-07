# WS63 Radio U0/U1 Facade Ownership Evidence

## Scope

This evidence closes the structural U0/U1 gates after BLE B3 and SLE S3. It
freezes the internal migration inputs and establishes one application-facing
`hisi-rf` composition lifecycle for BLE and SLE. It does not claim typed GAP,
GATT, announce, seek, SSAP, async event, cancellation, pairing, coexistence, or
stable API completion.

## Immutable Inputs

- `hisi-rf-ws63`: `2e0136c` (`test: freeze BLE and SLE migration surfaces`)
- `hisi-rf`: `9a600a8` (`feat: add BLE and SLE facade composition previews`)
- BLE archive profile: `ws63-ble-b0-archive-abi-v1`
- SLE archive profile: `ws63-sle-s0-archive-abi-v1`
- behavior evidence:
  [BLE B3 GATT](ws63-ble-b3-gatt-2026-08-07.md) and
  [SLE S3 SSAP](ws63-sle-s3-ssap-2026-08-07.md)

## U0 Contract

`hisi-rf-ws63` now keeps reviewed public-API snapshots for `ble-init` and
`sle-init`, plus host compile contracts that name the complete stage migration
inputs. The snapshots additionally prove that `BleB*` and `SleS*` remain
`#[doc(hidden)]`. The migration mapping records which B3/S3 behavior must move
to the future facade without declaring the stage API stable.

The same change fixed two host-tooling gaps discovered by the new gate: SLE's
public event UUID is now available to host rustdoc, and target-only BLE event
injection no longer fails host clippy as dead code.

## U1 Contract

`hisi-rf` adds mutually exclusive `profile-ble-dual-role` and
`profile-sle-ssap` previews. Both expose only facade-owned:

- caller-owned `RadioStorage` and installed storage capability;
- uniquely owned HAL `Resources`;
- `RadioController::split()`;
- the compile-time-selected `ble` or `sle` part;
- a mandatory `RadioRunner` that owns the internal WS63 stage controller.

The public-API gate rejects `hisi-rf-ws63`, `ws63-radio-sys`,
`hisi-rf-rtos-driver`, `BleB*`, and `SleS*` names. BLE and SLE profiles are also
compile-time mutually exclusive until coexistence has separate evidence.

## Verification

- BLE/SLE U0 host migration tests pass independently.
- BLE/SLE U0 host clippy passes with `-D warnings`.
- `ble-init` and `sle-init` each pass RV32 `cargo check -Zbuild-std=core,alloc`.
- BLE/SLE U1 facade ownership tests pass independently on the host.
- Both U1 profiles pass host clippy with `-D warnings`.
- Both U1 profiles pass RV32 `cargo check -Zbuild-std=core,alloc`.
- Wi-Fi, BLE U1, and SLE U1 public-API snapshots pass together.
- The BLE+SLE conflicting profile is rejected before build completion.

## Boundary

U1 deliberately does not expose operations on `BleController` or
`SleController`. The existing synchronous stage controllers still combine
vendor commands and event consumption, so forwarding them through a renamed
facade would falsely claim the runner boundary is complete. U2-U4 must add
typed commands, separate completion and unsolicited-event queues, generation
and cancellation semantics, and behavior parity before applications migrate.
No new firmware image was required for this structure-only change; B3/S3 fixed
image evidence remains the behavior baseline.
