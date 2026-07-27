# hisi-riscv-rs Roadmap

**North Star:** run reliable WS63 connectivity on real silicon: Wi-Fi scan,
connect, and sustained IP traffic through the Rust-visible data path.

HAL, RT, PAC, QEMU, HIL, image tooling, probe support, and documentation serve
that goal. This ecosystem is still moving quickly: when roadmap text, docs,
examples, and behavior disagree, prefer the latest passing CI/HIL evidence and
the stable API reference, then fix the documentation.

**WIP limit:** one major milestone at a time. A0-A4 are frozen; W2 pure-WPA3 is
externally blocked on a suitable SAE-only AP, so the active no-board milestone
is A5U developer UX and resource admission.

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
  ARP/neighbor cache, and repeated ICMP;
- parity with the frozen init/scan/connect/ping markers and A0/A3 link/image
  evidence.

The complete slice passes on WS63: WPA2 connect, a long-lived smoltcp
runner, DHCP, neighbor discovery, repeated public ICMP, zero RX-queue drops, and
an observed DHCP renew REQUEST/ACK. `hisi-rf 0.1.0-alpha.1` is published and the
committed self-hosted workflow is green. Evidence:
[A4 vertical slice](docs/plan/evidence/ws63-rf-a4-vertical-slice-2026-07-14.md).

## EXTERNAL BLOCKED -- W2 Upstream Supplicant And WPA3-Personal

The single execution ledger, current evidence, and W2A-W2F gates live in
[Connectivity stack W2](docs/plan/hisi-connectivity-stack.md#w2-upstream-supplicant-and-wpa3personal).
The active-window policy keeps the WPA2-only archive and A4 HIL gate green while
the upstream-native WPA3 path is introduced. The pinned upstream hostap path has
closed WPA2 connect/DHCP/public-ping/lease-renew parity, host protocol vectors,
and a 20/20 transition-mode SAE+required-PMF reset matrix. Its release unit is
published through tag CI, and the upstream WPA2/WPA3 firmware now performs one
stock-rust-lld link through ordinary Cargo on Linux, macOS and Windows. The same
plain-link image now passes an on-silicon init/non-empty-scan/native-runner gate;
full transition-mode connect parity, pure WPA3 SAE+PMF and migration retirement
remain open. Historical guarded linking is retained only for the vendor oracle.

BLE, SLE, TLS, SoftAP and Enterprise do not run in parallel with W2.

## NOW -- A5U Developer UX And Resource Admission

A5F has closed the single-dependency facade, normalized-link artifact and
macOS/Linux/Windows crates.io-only build path. A5U now makes resource ownership
and admission visible before radio startup: named profiles, caller-owned
storage, versioned machine-readable reports, actionable typed errors, and a
template happy path. Pure-WPA3 evidence remains an external gate; this no-board
work does not remove the vendor oracle or claim WPA3 stability.

Task-stack allocation is now observable and admitted before hardware startup:
the profile atomically reserves six 24 KiB stacks and six dynamic slots, while
the post-scan snapshot measured five live dynamic tasks at 24 KiB each. The
commit-state bootstrap HIL reported the full 144 KiB reservation and two
unconsumed stacks after initialization. Caller-owned stack storage, high-water
measurement, the shared supplicant arena, and whole-profile memory admission
remain open.
Evidence: [A5U task-stack allocation](docs/plan/evidence/ws63-rf-a5u-task-stack-allocation-2026-07-28.md).

## LATER -- Triggered Product Directions

After W2, choose exactly one direction from measured product demand:

- BLE for a concrete peripheral or central scenario;
- NVS N0-N3 when the release image must stop depending on the vendor NV
  generator;
- TLS after stable TCP/IP plus an HTTP/MQTT consumer exists;
- SLE after a second WS63 rig and a concrete interconnect scenario exist.

WPA3-Personal was selected at the A4 product gate; the remaining choices stay
triggered rather than pre-booked as concurrent work.

## DEFERRED -- Triggered Backlog

These designs remain documented, but are not active TODO checklists:

- ported switch ticket/generation protocol, after A3; keep the verified stale
  switch recovery until its 100-reset parity gate passes;
- group Reservation and guaranteed-service scheduling;
- RTOS protection/PMP/TES/SMP, host replay, `hisi-rtos-cli`, and IDE support;
- NVS factory/write/GC/encryption and complete hardware key-slot/crypto support;
- Enterprise Wi-Fi, SoftAP, BLE, SLE, and coexistence;
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
