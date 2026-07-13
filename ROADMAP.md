# hisi-riscv-rs Roadmap

**North Star:** run WS63 connectivity on real silicon: Wi-Fi scan, connect, then ping.

HAL, RT, PAC, QEMU, HIL, image tooling, probe support, and documentation are support systems for that connectivity goal, not goals by themselves.

This ecosystem is still moving quickly. If roadmap text, docs, examples, or local behavior disagree, prefer the latest passing CI/HIL result and the stable API reference, then update the docs.

## Current State

**Done and usable as the baseline:** official Rust nightly target path, `hisi-riscv-rt` chip adapters, HAL `0.6.0` stable/unstable gating, WS63 embedded-test HIL, QEMU smoke coverage, `hisi-fwpkg` image planning, and WS63 Wi-Fi init/scan/open/WPA2 connect/DHCP/ARP/ping through the Rust-visible L2 path.

**Active focus:** preserve the frozen C5/A0 connectivity baseline while starting C6 ownership decomposition. HAL 0.6.0 stabilization and the H0 `hisi-hal` rename are complete; every extraction must reproduce the same scan/connect/ping markers and link/image evidence.

**Not a near-term target:** BSP/board-manager, BS2X BLE/SLE real-board connectivity, Hi3322 runtime implementation, DMA stable graduation, and embassy stable graduation. These are deferred until they serve the connectivity path or have hardware evidence.

**Current plans live in:** [HAL 0.6.0 release plan](docs/plan/hal-0.6.0-release.md), [WS63 RF init+scan/RF5 plan](docs/plan/ws63-rf-init-scan.md), and the [HiSilicon Connectivity full-stack plan](docs/plan/hisi-connectivity-stack.md).

**Current facts live in:** [Stable API 清单](docs/src/reference/10-stable-api.md), [`ws63-rf-rs` README](chips/ws63/rf/README.md), [ws63-RF 组件文档](docs/src/explanation/components/09-ws63-rf.md), and the archived [2026-05 to 2026-07 remediation roadmap](docs/archive/roadmap-2026-05-2026-07-remediation.md).

## Connectivity Milestones

| Milestone | Goal | Acceptance |
| --- | --- | --- |
| C0 Baseline locked | Keep the current toolchain, HAL alpha, RT, fwpkg, probe-rs baseline, QEMU smoke, and WS63 HIL path usable while connectivity work proceeds. | `uart_hello`/HIL smoke and docs happy path continue to pass; no widening of stable HAL surface without named HIL evidence. |
| C1 RF runtime image (done) | Link `ws63-rf-rs` plus the real Wi-Fi blob set into a flashable WS63 image. Add the real `.wifi_pkt_ram` NOLOAD region instead of ad hoc symbol scaffolding. | Image builds, packages through `hisi-fwpkg`, boots far enough to print RF bring-up markers, and QEMU RF selftests remain green. |
| C2 Wi-Fi init on silicon (done) | Call the minimal `wifi_init` path on a real board. Make failures distinguishable as ROM symbol, relocation, NV/eFuse, RF clock, IRQ, or memory-layout faults. | UART/HIL captures a deterministic `wifi_init` pass/fail marker with structured reason codes. |
| C3 Scan (done) | Enable STA scan and return AP results or a precise RF/NV failure. | Add a `wifi_scan` example and HIL marker; scan either prints at least one AP in a controlled environment or reports a known categorized failure. |
| C4 Connect (done) | Associate to a controlled open or WPA2 test AP. | Connection state transitions and failure codes are observable over UART/HIL. |
| C5 Ping (done) | Complete an IP round trip over the Rust-visible network path, using smoltcp or the vendor netif boundary chosen by the bring-up evidence. | Connectivity HIL reproduced ICMP Echo to `1.1.1.1` on real WS63 silicon; see the [A0 baseline](docs/plan/evidence/ws63-rf-a0-2026-07-12.md). |
| C6 Architecture baseline (H0 done) | The `hisi-riscv-hal` → `hisi-hal` rename is complete. Next split ROM, blob sys, allocator, storage/NVS, RTOS-driver, RTOS, and high-level RF ownership without regressing C5. | HAL rename checks show no API drift; frozen scan/connect/ping markers and link/image reports pass through the new dependency graph. |
| C7 BLE | Bring up the vendor BLE host through the shared RTOS/storage/runtime contracts. | Advertising, scanning, and GATT client/server have bounded-event APIs and real-board evidence. |
| C8 SLE and coexistence | Bring up SLE, then validate concurrent Wi-Fi plus BLE/SLE operation. | Two-board SLE data exchange passes; `coex` remains hidden until concurrent HIL passes. |
| C9 Connectivity release | Turn the full stack into repeatable release units. | Compatibility/resource matrices, release notes, known issues, examples, docs, and HIL evidence are aligned. |

## Maintenance Tracks

**HAL 0.6.0 stabilization (complete):** the stable release is published. The renamed `hisi-hal 0.7.0-alpha.1` preserves that stable surface; DMA, embassy, BS2X, and unproven helper surfaces remain behind `unstable` until their invariants and HIL evidence are closed.

The detailed HAL release gate is tracked in [docs/plan/hal-0.6.0-release.md](docs/plan/hal-0.6.0-release.md). RF/Connectivity may drive HAL bug fixes, but does not block HAL 0.6.0 unless it exposes a bug in an already-stable HAL API.

Init/scan evidence and RF5 are tracked in [docs/plan/ws63-rf-init-scan.md](docs/plan/ws63-rf-init-scan.md). The post-ping component architecture, RTOS/NVS split, BLE/SLE and coexistence sequence are tracked in [docs/plan/hisi-connectivity-stack.md](docs/plan/hisi-connectivity-stack.md).

Deferred after the connectivity baseline: the portable/protected RTOS and CLI-first debugging outlook is tracked in [docs/plan/hisi-rtos-future-architecture.md](docs/plan/hisi-rtos-future-architecture.md); it does not block current RF milestones.

**Probe, fwpkg, and toolchain:** make only the changes required to keep connectivity work reproducible. `hisi-fwpkg` remains the image-format fact source; probe-rs should stay a generic transport/debug path and avoid HiSilicon image-format parsing.

Deferred read-only investigation of additional WS63 system-memory debug paths is tracked in [docs/plan/ws63-debug-memory-access.md](docs/plan/ws63-debug-memory-access.md); it is not on the connectivity critical path.

**Docs and CI:** preserve the happy path, stable API reference, and connectivity roadmap. Avoid expanding component documentation unless it prevents drift or directly supports C1-C6.

Low-priority i18n track: integrate `mdbook-i18n-helpers` after the current Chinese handbook, chip selector, version selector, snippet preprocessor, and happy-path CI stay stable. The intended shape is gettext-style extraction/translation around the existing mdBook source, without forking the command snippets or reference facts into per-language copies. Acceptance: `mdbook-xgettext`/`mdbook-gettext` are wired into a documented script or CI check, translated pages reuse the same chip/version metadata and snippet contracts, and untranslated pages degrade back to the canonical Chinese source. This track must not block connectivity milestones C1-C6.

**BSP and board-manager:** deferred. Revisit after real connectivity works and after external boards or user projects create enough pressure for board manifests or board-selection tooling.

## Historical Context

The original roadmap was created from the 2026-05 architecture review and grew into a remediation ledger covering build integrity, HIL bring-up, runtime architecture, HAL API tightening, RF porting, docs, release, and tooling work. That record is archived at [`docs/archive/roadmap-2026-05-2026-07-remediation.md`](docs/archive/roadmap-2026-05-2026-07-remediation.md).

Review ledgers remain under [`docs/review/`](docs/review/). They preserve the evidence trail, but they do not override the current priority order above.
