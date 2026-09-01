# hisi-riscv-rs Roadmap

**North Star:** run reliable WS63 connectivity on real silicon: Wi-Fi scan,
connect, and sustained IP traffic through the Rust-visible data path.

HAL, RT, PAC, QEMU, HIL, image tooling, probe support, and documentation serve
that goal. This ecosystem is still moving quickly: when roadmap text, docs,
examples, and behavior disagree, prefer the latest passing CI/HIL evidence and
the stable API reference, then fix the documentation.

**WIP limit:** one major milestone at a time. A0-A5, the repository-owned
two-WS63 WPA2/WPA3 release gate, BLE B0-B3, SLE S0-S3, and Radio UX U0-U4 are
frozen. U5A-U5C security control, hardware-crypto compatibility, and
vendor-managed bond persistence/restore/removal, U5D positive/negative pairing
lifecycles, and U6 named-profile/template delivery are also frozen. The current
major WIP is U7: crates.io-only external consumers plus measured two-board
coexistence acceptance.

## Completed -- A3 Runtime And Connectivity Baseline

The frozen A3 evidence establishes:

- the 20-reset matrix completed association, DHCP and ARP 20/20 with no
  `0x1451` authentication timeout or scheduler corruption;
- Q3 binds observed vendor tasks to the pinned archive and Q4 keeps them
  Cooperative without group quota or Reservation;
- the bounded Rust RX ring had zero queue-full drops and at most one of four
  slots occupied in the diagnostic matrix;
- a Mac forced through the same Guest AP reproduced gateway silence and the
  exact public `88/100` (12% loss) result, giving the capability proof a
  quantified environmental boundary.

Evidence: [task profile and multi-ping](docs/plan/evidence/ws63-rf-a3-task-profile-multiping-2026-07-14.md)
and [network attribution](docs/plan/evidence/ws63-rf-a3-network-attribution-2026-07-14.md).
Changed payloads, APs, routes or task sets must be measured again rather than
inheriting this boundary.

## Completed -- A4 Wi-Fi Vertical Slice

A4 delivered one coherent Wi-Fi path:

- `RadioController` / `RadioRunner`;
- separate `WifiController` and `WifiDevice`;
- a bounded event queue with no user callbacks in IRQ or critical sections;
- a long-lived smoltcp or embassy-net runner covering lease renewal,
  ARP/neighbor cache, and routed UDP DNS;
- parity with the frozen init/scan/connect/ping markers and A0/A3 link/image
  evidence.

The complete slice passes on WS63: WPA2 connect, a long-lived smoltcp
runner, DHCP, direct ARP reply evidence, validated public UDP DNS responses,
zero RX-queue drops, and
an observed DHCP renew REQUEST/ACK. `hisi-rf 0.1.0-alpha.1` is published and the
committed self-hosted workflow is green. Evidence:
[A4 vertical slice](docs/plan/evidence/ws63-rf-a4-vertical-slice-2026-07-14.md).

## Completed -- W2 Upstream Supplicant And WPA3-Personal

The single execution ledger, current evidence, and W2A-W2F gates live in
[Connectivity stack W2](docs/plan/hisi-connectivity-stack.md#w2-upstream-supplicant-and-wpa3personal).
The active-window policy keeps the WPA2-only archive and A4 HIL gate green while
the upstream-native WPA3 path is introduced. The pinned upstream hostap path has
closed WPA2 connect/DHCP/public-ping/lease-renew parity, host protocol vectors,
and a 20/20 transition-mode SAE+required-PMF reset matrix. Its release unit is
published through tag CI, and the upstream WPA2/WPA3 firmware now performs one
stock-rust-lld link through ordinary Cargo on Linux, macOS and Windows. The same
plain-link image now passes an on-silicon init/non-empty-scan/native-runner gate;
transition-mode connect parity is closed. Matching repository-owned Rust WPA3
SoftAP and upstream-native WPA3 STA profiles now run a fixed non-production fixture.
Across the original three unchanged-image 20-reset matrices,
SAE/PMF/association/DHCP completed 60/60 with no authentication-response-2 timeout,
but one matrix recorded two local echo reply-path failures. Later targeted matrices
reproduced partial reply loss and drove socket-capacity and RTOS ownership fixes.
The final pinned release closure passes separate WPA2 and pure-WPA3 20-reset matrices,
each with 200/200 local echo and zero authentication, backend, queue or ownership
errors. Historical failures remain evidence and are not assigned to one unproven cause. Evidence:
[dual-board pure-WPA3 reliability](docs/plan/evidence/ws63-rf-dual-board-pure-wpa3-reliability-2026-08-04.md).
Migration retirement remains version-gated: one parent migration release must exist,
and `ws63-rf-rs` removal is not earlier than parent v0.8.0.
Historical guarded linking is retained only for the vendor oracle.

BLE, SLE, TLS, Enterprise and broader user-facing SoftAP productization do not run
in parallel with W2; the bounded SoftAP used as the HIL peer is part of this gate.

## Completed -- A5 Correctness And Release Closure

A5F has delivered the single-dependency facade, normalized-link artifact and
macOS/Linux/Windows crates.io-only build path. A5U exposes named profiles,
caller-owned storage, versioned machine-readable reports, actionable typed
errors, and a template happy path. A5B now keeps start/cancel in bounded
in-memory transitions and advances vendor work only through budgeted poll turns;
A5F has removed hidden backend and concrete RTOS types from public signatures.
The incremental cancellation model, public native-supplicant disconnect path,
upstream hostap key cleanup, and WS63 WAL key programming/rollback are now
covered across their exact production seams. Credential-free cancellation and
timeout injection now also crosses the public controller, facade channels,
incremental runner, and WS63 backend, with identical QEMU and silicon output.
The public `wifi_connectivity` example now composes the facade, incremental
runner, scan/connect, DHCP, direct ARP evidence, redundant public UDP DNS and lease renewal
in one final ELF.
The response-bound and full connectivity scripts build immutable profile-specific
images from the same release closure; WPA2 and pure-WPA3 are not one ELF. The
pinned release-candidate closure now passes the fail-closed marker-contract-v2
QEMU fixture plus separate WPA2 and pure-WPA3 two-board 20-reset matrices. Both
profiles reached 20/20 with 200/200 local echo replies, zero authentication
timeout, queue drop, or backend error. Historical ICMP evidence remains dated
evidence rather than the current pass/fail contract. Exact artifacts and the
remaining causality boundary are recorded in the
[release-closure evidence](docs/plan/evidence/ws63-rf-release-closure-wpa2-wpa3-2026-08-06.md).

The application migration contract is documented in
[`ws63-rf-rs` to `hisi-rf`](docs/src/how-to/12-migrate-ws63-rf-to-hisi-rf.md).
`hisi-rf-ws63 0.1.0-alpha.71` and `hisi-rf 0.1.0-alpha.83` make the bounded runner
the named-profile default; the blocking backend remains only behind an explicit
migration feature. This does not remove the vendor oracle, claim
WPA3 stability, or start BLE/SLE/TLS/NVS work while the A5 window is active.

Task-stack and shared-arena admission are now closed before hardware startup:
the profile atomically reserves six 24 KiB stacks and six dynamic slots, while
the 296 KiB caller-owned arena is installed once against a fixed 544 KiB WS63
memory profile. The final linker contract reserves a 32 KiB radio main stack,
checks overlap at link time, and passed 3 MHz full-verify init/non-empty-scan
HIL with no event drop or backend error.
Evidence: [A5U task-stack allocation](docs/plan/evidence/ws63-rf-a5u-task-stack-allocation-2026-07-28.md).
The shared-arena closure is recorded in
[shared RF arena evidence](docs/plan/evidence/ws63-rf-a5u-shared-arena-2026-07-28.md).
Resource shortage, association rejection, first-EAPOL timeout, operation
cancellation and backend timeout now emit the same production
`hisi-rf-error/v3` diagnostics in host fixtures, QEMU and on real WS63.
Operation cancellation and timeout also pass through the production
public controller and incremental state machine, then return through facade
completion/event channels while proving terminal-slot recovery. A
production-path host regression now covers key-held cancellation, one retained
replacement, late success suppression, timer cleanup and generation-safe slot
reuse.
The protocol, backend and application waiting boundaries now use distinct
`OperationTimeout`, `BackendTimeout` and application-owned deadline types.
Template `v0.7.0-alpha.15` now generates the Wi-Fi firmware, FlashPlan image
and deterministic profile resource report from the same public `hisi-rf`
dependency. Generated projects carry their own pinned toolchain, target/linker
and release-profile configuration; tag-gated native Linux, macOS and Windows CI
all pass before the GitHub prerelease is published.
Evidence: [A5U operation error injection](docs/plan/evidence/ws63-rf-a5u-operation-error-injection-2026-07-28.md).
Resource-conservation evidence:
[A5 incremental resource conservation](docs/plan/evidence/ws63-rf-a5-resource-conservation-2026-07-28.md).

## Completed -- B0 BLE Archive And ABI Closure

B0 fixes the exact WS63 BLE archive/profile inputs before any runtime or user API
is added. It inventories the vendor controller/host/GAP/GATT/SMP closure, pins
archive hashes, generates the undefined-symbol manifest, and maps each symbol to
its RTOS, NVS, crypto, allocator, transport or coexistence owner. CI must fail on
an unreviewed symbol or ABI/layout change. The contract is published as
`ws63-radio-sys 0.1.0-alpha.12`; evidence is recorded in
[BLE B0 archive/ABI closure](docs/plan/evidence/ws63-ble-b0-archive-abi-2026-08-06.md).

## Completed -- B1 BLE Controller And Host Init

B1 resolves only the fixed B0 archives' controller/host initialization closure:
shared transport, NVS identity/bonding reads, allocator and RTOS capabilities,
resource admission, and an on-silicon init marker. Advertising, scanning, GATT,
SLE/GLE, coexistence, and public BLE API remain outside this window.

## Completed -- B2 BLE Advertising And Scanning

B2 adds only advertising, scanning, a bounded event queue, and real-silicon
markers on top of the B1 controller/host baseline. The fixed release ELF passed
a paired two-board 20-reset discovery matrix with no callback, command, or event
queue failures. Evidence is recorded in
[BLE B2 advertising/scanning](docs/plan/evidence/ws63-ble-b2-advertising-scanning-2026-08-07.md).

## Completed -- B3 BLE GATT

B3 delivered GATT client/server, notification/indication, and disconnect
cleanup in a fixed-image paired 20-reset matrix. Evidence is recorded in
[BLE B3 GATT](docs/plan/evidence/ws63-ble-b3-gatt-2026-08-07.md).

## Completed -- S0-S3 SLE Archive, Connection And SSAP

S0 pins the redistributable normalized SLE archives, symbol/ABI ownership and
cross-platform Cargo contract. S1 adds independent `enable_sle` initialization,
announce/seek, bounded copied events, and a paired two-board 20-reset matrix.
S2 adds connect/disconnect state and bounded connection events. S3 adds SSAP
exchange, service discovery, property read, server notification, and clean
disconnect; it deliberately does not claim pairing, authenticated SSAP, or
client write.
Evidence is recorded in
[SLE S0 archive/ABI closure](docs/plan/evidence/ws63-sle-s0-archive-abi-2026-08-07.md)
and [SLE S1 announce/seek](docs/plan/evidence/ws63-sle-s1-announce-seek-2026-08-07.md),
plus [SLE S2 connect/disconnect](docs/plan/evidence/ws63-sle-s2-connect-disconnect-2026-08-07.md)
and [SLE S3 SSAP](docs/plan/evidence/ws63-sle-s3-ssap-2026-08-07.md).

## NOW -- U7 External Consumer And Coexistence Acceptance

The product gate selected Radio UX/API convergence. U0 has frozen the B3/S3
migration inputs and U1 now provides facade-owned BLE/SLE storage,
`RadioController`, compile-time protocol parts, and `RadioRunner` ownership.
U2 typed BLE GAP and SLE announce/seek control has passed separate two-board
20-reset matrices, and U3 static typed GATT/SSAP databases are complete. The
U4 closes bounded lifecycle events, generation-tagged active guards, and
explicit/best-effort cancellation. Its host/API/RV32 gate and the BLE/SLE
two-board 20-reset lifecycle matrices are complete. U5A/U5B then closed the
typed security control plane and required hardware crypto boundary. Fixed U5
peripheral/central images now pass the original pairing/observation gate and the
released `hisi-rf 0.1.0-alpha.90` images pass a restored-bond `3/3` gate plus a
`20/20` paired nRST matrix. Both boards restored the vendor-managed bond in all
20 runs, restarted the current connection's security procedure, and reached a
typed `Paired` state without failure markers. Persistence and restore are
therefore frozen. U5C also completed `remove -> NotPaired -> reset -> BOND_EMPTY`
with copy-first/header-last NVS GC and an alternating 20-reset matrix; deletion
survives reset rather than only changing the current in-memory table. Evidence:
[initial pairing and observation](docs/plan/evidence/ws63-radio-u5-pairing-bond-observation-2026-08-12.md),
[bond restore across reset](docs/plan/evidence/ws63-radio-u5c-bond-restore-2026-08-13.md),
and [bond removal and NVS GC](docs/plan/evidence/ws63-radio-u5c-bond-removal-gc-2026-08-20.md).

U5D positive Secure Connections now also passes a fresh-pair `3/3` gate and an
unchanged-image restored-bond `20/20` paired nRST matrix. The production bridge
uses the Rust WS63 P-256 backend plus a health-checked DRBG for archive-facing
key generation and ECDH, fails closed without software fallback, and preserves
the vendor ROM scratch lifecycle. Evidence:
[Secure Connections pairing and restore](docs/plan/evidence/ws63-radio-u5d-secure-connections-2026-08-24.md).

U5D negative pairing is also complete. Reject and stale-generation fixtures each
passed a `3/3` shape gate and an unchanged-image `20/20` paired reset matrix with
persistent `BOND_EMPTY`. The schema-7 runner physically holds central nRST while
the peripheral settles and rejects boot-before-protocol or multiple-generation
logs; this replaces an invalid earlier matrix whose records could contain prior-
generation markers. Evidence:
[negative pairing lifecycles](docs/plan/evidence/ws63-radio-u5d-negative-pairing-2026-08-29.md).

U5D is therefore closed. U6 packages the resulting API into six named
BLE/SLE profiles, caller-owned storage, deterministic machine-readable resource
reports, and generated projects that depend only on the public `hisi-rf` facade.
The backend facts are released in `hisi-rf-ws63 0.1.0-alpha.86`; the facade and
JSON report contract are released in `hisi-rf 0.1.0-alpha.101`.
`hisi-rs-template v0.7.0-alpha.30` generates all six profiles, builds the RV32
projects and representative images, and emits Wi-Fi/BLE/SLE reports on native
Linux, macOS and Windows. U6 is therefore closed.

U7 remains the single WIP. Its crates.io-only consumer gate now passes on all
three host OSes, and fixed Wi-Fi+BLE/Wi-Fi+SLE images pass the shared-init 3/3
and 20/20 reset gates. The Wi-Fi local-UDP + BLE-advertising lane now also passes
3/3 and 20/20 paired reset gates with 200/200 unique sequences recovered; its
bounded retry and RAM costs remain visible in the
[evidence](docs/plan/evidence/ws63-radio-u7-wifi-ble-traffic-2026-08-31.md).
The Wi-Fi local-UDP + SLE-announcing lane now also passes 3/3 and 20/20 paired
reset gates with 200/200 unique sequences recovered and zero Wi-Fi/SLE event
drops; its retry and RAM costs remain visible in the
[evidence](docs/plan/evidence/ws63-radio-u7-wifi-sle-traffic-2026-09-01.md).
The SLE-connected lane now also passes 3/3 and 20/20 paired reset gates while
maintaining the SLE link, completing three scans, associating over WPA2, and
recovering 200/200 unique UDP sequences; see the
[connected-SLE evidence](docs/plan/evidence/ws63-radio-u7-wifi-sle-connected-traffic-2026-09-01.md).
The BLE-connected lane now also passes 3/3 and 20/20 paired reset gates, with
20/20 BLE links, 20/20 WPA2 associations, and exactly 200/200 unique UDP
sequences; see the
[connected-BLE evidence](docs/plan/evidence/ws63-radio-u7-wifi-ble-connected-traffic-2026-09-01.md).
The remaining gate is explicit event conservation plus measured IRQ/resource
latency and runtime watermark acceptance. No stable `coex` promise is public
before that gate passes. DLI/SLB and stable graduation remain deferred.

## LATER -- Triggered Product Directions

After B0-B3, choose exactly one direction from measured product demand:

- NVS N0-N3 when the release image must stop depending on the vendor NV
  generator;
- TLS after stable TCP/IP plus an HTTP/MQTT consumer exists;
- coexistence only after S3 SSAP is reproducible on the two-board rig.

WPA3-Personal was selected at the A4 product gate; the remaining choices stay
triggered rather than pre-booked as concurrent work.

## DEFERRED -- Triggered Backlog

These designs remain documented, but are not active TODO checklists:

- ported switch ticket/generation protocol, after A3; keep the verified stale
  switch recovery until its 100-reset parity gate passes;
- group Reservation and guaranteed-service scheduling;
- RTOS protection/PMP/TES/SMP, host replay, `hisi-rtos-cli`, and IDE support;
- NVS factory/write/GC/encryption and complete hardware key-slot/crypto support;
- Enterprise Wi-Fi, broader SoftAP productization, pairing UX, BLE/SLE typed
  metadata/schema graduation, stable BLE/SLE API, DLI/SLB productization,
  Wi-Fi L2/Embassy Net/TLS/application-protocol ecosystem closure
  ([NET0-NET5](docs/plan/hisi-connectivity-stack.md#wi-fi-上层生态补全net0-net5延期)),
  and coexistence;
- BSP/board-manager, mdBook i18n, Hi3322 runtime, and AP1 probe-rs fast-path
  integration.

Detailed deferred facts stay in their owning plans:

- [RTOS semantics and verification](docs/plan/hisi-rtos-semantics-and-verification.md)
- [RTOS future architecture](docs/plan/hisi-rtos-future-architecture.md)
- [RTOS observability and CLI](docs/plan/hisi-rtos-debugging-cli.md)
- [`cargo-hisi` developer workflow CLI](docs/plan/cargo-hisi-cli.md)
- [NVS image tooling](docs/plan/hisi-nvs-image.md)
- [WS63 debug memory diagnosis](docs/plan/ws63-debug-memory-access.md)

Completed historical plans remain as evidence, not current work:

- [HAL 0.6.0 release](docs/plan/hal-0.6.0-release.md)
- [WS63 RF init/scan/RF5](docs/plan/ws63-rf-init-scan.md)

## Fact Sources

- plan status, priority, triggers, and dependencies: [Engineering plan registry](docs/plan/README.md)
- current execution and architecture: [Connectivity stack](docs/plan/hisi-connectivity-stack.md)
- stable HAL surface: [Stable API reference](docs/src/reference/10-stable-api.md)
- transitional RF implementation: [`ws63-rf-rs` README](chips/ws63/rf/README.md)
- RF architecture explanation: [WS63 RF component](docs/src/explanation/components/09-ws63-rf.md)
- historical remediation ledger:
  [2026-05 to 2026-07 archive](docs/archive/roadmap-2026-05-2026-07-remediation.md)

Review ledgers under [`docs/review/`](docs/review/) preserve dated evidence; they
do not override the current priority order or current reference facts.
