# hisi-rf U8 Stable-Graduation Review -- 2026-09-01

## Scope

This review maps the released `hisi-rf 0.1.0-alpha.110` facade to the completed
U0-U7 evidence. It is an API graduation decision, not a replay of the fixed
BLE/SLE coexistence HIL matrices and not a claim that external RF traffic can
never be lost.

Inputs:

- `hisi-rf` public API snapshots for WS63 Wi-Fi, BLE U5, and SLE U4;
- crates.io-only Linux, macOS, and Windows consumer gates;
- the U7 connected BLE/SLE 3-reset and 20-reset matrices;
- resource-report schema v13 and schema-3 event/resource acceptance;
- `hisi-rf` commit `e556093`, which makes this decision executable through
  `.github/stable-graduation.toml` and
  `.github/scripts/check-stable-graduation.py`.

## Decision

No public RF surface graduates in U8. Existing alpha APIs remain available and
their evidence remains valid, but the stable promise would currently expose
implementation details or incomplete lifecycle contracts.

| Surface | Evidence already closed | Graduation decision | Blocking facts |
|---|---|---|---|
| Wi-Fi station | WPA2/WPA3, bounded runner, crates.io-only three-OS consumer, two-board traffic | Blocked | The public snapshot still contains 16 `hisi_rf_ws63` signatures; allocator hooks are public unsafe functions; NET0/NET1 link metadata and Embassy Net contract remain deferred. |
| BLE | Typed role profiles, lifecycle cancellation, GATT, security/bonding, two-board U7 acceptance | Blocked | Backend crate types are hidden, but public events still expose raw `stage: u8` and `status: u32`; runtime allocator hooks remain public unsafe functions; migration naming has not completed final review. |
| SLE | Typed announce/seek, lifecycle cancellation, SSAP server slice, two-board U7 acceptance | Blocked | Backend crate types are hidden, but raw backend event integers and public unsafe allocator hooks remain; connection and SSAP client lifecycle are not represented by the current facade slice. |
| Coexistence | Connected Wi-Fi+BLE and Wi-Fi+SLE fixed-image acceptance | Hidden | Only `#[doc(hidden)] __coexistence` maintainer fixtures exist. There is no public shared `RadioController` lifecycle, capability negotiation, or recovery contract. |

## Evidence Boundary

The U7 BLE and SLE lanes each passed 3/3 plus 20/20 paired nRST. Across 80 role
snapshots, `accepted = consumed + pending`, event drops and allocation failures
were zero, and RTOS ready-ownership diagnostics remained clean. Those results
prove the fixed integration artifacts under the recorded environment. They do
not define a stable public API, prove mathematical liveness, or justify exposing
the maintainer coexistence fixture.

## Follow-Up

The next and only major WIP is U8R facade-boundary remediation:

1. replace raw BLE/SLE backend stage/status fields with typed, actionable facade
   errors while retaining optional vendor diagnostics;
2. move runtime allocation behind a capability owned by the composition root so
   normal applications do not call public unsafe allocator hooks;
3. wrap Wi-Fi resources, storage, diagnostics, device, init error, and runner
   wait diagnostics so no chip-backend type appears in public signatures;
4. rerun API snapshots, compile-fail/host tests, three-OS crates.io-only
   consumers, documentation, and the relevant fixed-artifact HIL before a new
   graduation decision.

Coexistence, full SLE client UX, Embassy Net, DLI/SLB, and stable protocol APIs
remain separate evidence gates. U8R must not widen into those product areas.
