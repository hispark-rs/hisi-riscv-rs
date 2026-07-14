# hisi-riscv-rs Roadmap

**North Star:** run reliable WS63 connectivity on real silicon: Wi-Fi scan,
connect, and sustained IP traffic through the Rust-visible data path.

HAL, RT, PAC, QEMU, HIL, image tooling, probe support, and documentation serve
that goal. This ecosystem is still moving quickly: when roadmap text, docs,
examples, and behavior disagree, prefer the latest passing CI/HIL evidence and
the stable API reference, then fix the documentation.

**WIP limit:** one major milestone at a time. The active milestone is A3; A4
starts only after the A3 closeout evidence is frozen.

## NOW -- Close A3

The single active execution source is the
[Connectivity stack Active Window](docs/plan/hisi-connectivity-stack.md#active-window-now-a3-next-a4).
Current work is limited to four outcomes:

1. **Network reliability attribution:** run 3-5 pings per reset, record TX/RX,
   RTT, drops, and loss rate, and compare gateway behavior with a reference host.
   C5 capability proof is complete; the current 18/20 public-ping result is a
   data-plane reliability risk, not an authentication regression.
2. **Q3 archive-bound task profile:** record only tasks created by the pinned
   archive, including archive hash, entry symbol, vendor priority, Q2 metrics,
   and `critical`/`worker`/`background`/`unknown` role. Classification does not
   imply a policy change.
3. **Q4 decision:** use Q2 evidence to decide whether any task needs a per-thread
   `Budgeted` policy. If Cooperative remains stable, record that group quota is
   currently unnecessary. Reservation is not implemented without a measured
   service guarantee requirement.
4. **Freeze A3:** preserve reset statistics, scheduler invariants, versions,
   submodule pointers, profile revision, and the resulting quota decision.

A3 is complete only when scheduler invariants and the RF connection matrix are
stable, the task profile has machine-readable facts, and the quota decision is
explicit.

## NEXT -- A4 Wi-Fi Vertical Slice

A4 delivers one coherent Wi-Fi path:

- `RadioController` / `RadioRunner`;
- separate `WifiController` and `WifiDevice`;
- a bounded event queue with no user callbacks in IRQ or critical sections;
- a long-lived smoltcp or embassy-net runner covering lease renewal,
  ARP/neighbor cache, and repeated ICMP;
- parity with the frozen init/scan/connect/ping markers and A0/A3 link/image
  evidence.

A4 does not run BLE, SLE, TLS, SoftAP, or another architecture extraction in
parallel.

## LATER -- One Product Direction

After A4, choose exactly one direction from measured product demand:

- WPA3-Personal/SAE for modern Wi-Fi security;
- BLE for a concrete peripheral or central scenario;
- NVS N0-N3 when the release image must stop depending on the vendor NV
  generator;
- TLS after stable TCP/IP plus an HTTP/MQTT consumer exists;
- SLE after a second WS63 rig and a concrete interconnect scenario exist.

The default recommendation is WPA3-Personal, but that choice is made at the A4
product gate rather than pre-booked as concurrent work.

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
- [NVS image tooling](docs/plan/hisi-nvs-image.md)
- [WS63 debug memory diagnosis](docs/plan/ws63-debug-memory-access.md)

Completed historical plans remain as evidence, not current work:

- [HAL 0.6.0 release](docs/plan/hal-0.6.0-release.md)
- [WS63 RF init/scan/RF5](docs/plan/ws63-rf-init-scan.md)

## Fact Sources

- current execution and architecture: [Connectivity stack](docs/plan/hisi-connectivity-stack.md)
- stable HAL surface: [Stable API reference](docs/src/reference/10-stable-api.md)
- transitional RF implementation: [`ws63-rf-rs` README](chips/ws63/rf/README.md)
- RF architecture explanation: [WS63 RF component](docs/src/explanation/components/09-ws63-rf.md)
- historical remediation ledger:
  [2026-05 to 2026-07 archive](docs/archive/roadmap-2026-05-2026-07-remediation.md)

Review ledgers under [`docs/review/`](docs/review/) preserve dated evidence; they
do not override the current priority order or current reference facts.
