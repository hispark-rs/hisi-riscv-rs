# hisi-riscv-rs Roadmap

**North Star:** run WS63 connectivity on real silicon: Wi-Fi scan, connect, then ping.

HAL, RT, PAC, QEMU, HIL, image tooling, probe support, and documentation are support systems for that connectivity goal, not goals by themselves.

This ecosystem is still moving quickly. If roadmap text, docs, examples, or local behavior disagree, prefer the latest passing CI/HIL result and the stable API reference, then update the docs.

## Current State

**Done and usable as the baseline:** official Rust nightly target path, `hisi-riscv-rt` chip adapters, HAL `0.6.0-alpha.2` stable/unstable gating, WS63 embedded-test HIL, QEMU smoke coverage, `hisi-fwpkg` image planning, and the `ws63-rf-rs` porting runtime.

**Active focus:** turn the RF porting runtime into a real WS63 Wi-Fi image on hardware. The immediate work is real blob execution, `.wifi_pkt_ram`, pbuf/TX sink alignment, Wi-Fi init, and scan.

**Not a near-term target:** BSP/board-manager, BS2X BLE/SLE real-board connectivity, Hi3322 runtime implementation, DMA stable graduation, and embassy stable graduation. These are deferred until they serve the connectivity path or have hardware evidence.

**Current facts live in:** [Stable API 清单](docs/src/reference/10-stable-api.md), [`ws63-rf-rs` README](chips/ws63/rf/README.md), [ws63-RF 组件文档](docs/src/explanation/components/09-ws63-rf.md), and the archived [2026-05 to 2026-07 remediation roadmap](docs/archive/roadmap-2026-05-2026-07-remediation.md).

## Connectivity Milestones

| Milestone | Goal | Acceptance |
| --- | --- | --- |
| C0 Baseline locked | Keep the current toolchain, HAL alpha, RT, fwpkg, probe-rs baseline, QEMU smoke, and WS63 HIL path usable while connectivity work proceeds. | `uart_hello`/HIL smoke and docs happy path continue to pass; no widening of stable HAL surface without named HIL evidence. |
| C1 RF runtime image | Link `ws63-rf-rs` plus the real Wi-Fi blob set into a flashable WS63 image. Add the real `.wifi_pkt_ram` NOLOAD region instead of ad hoc symbol scaffolding. | Image builds, packages through `hisi-fwpkg`, boots far enough to print RF bring-up markers, and QEMU RF selftests remain green. |
| C2 Wi-Fi init on silicon | Call the minimal `wifi_init` path on a real board. Make failures distinguishable as ROM symbol, relocation, NV/eFuse, RF clock, IRQ, or memory-layout faults. | UART/HIL captures a deterministic `wifi_init` pass/fail marker with structured reason codes. |
| C3 Scan | Enable STA scan and return AP results or a precise RF/NV failure. | Add a `wifi_scan` example and HIL marker; scan either prints at least one AP in a controlled environment or reports a known categorized failure. |
| C4 Connect | Associate to a controlled open or WPA2 test AP. | Connection state transitions and failure codes are observable over UART/HIL. |
| C5 Ping | Complete an IP round trip over the Rust-visible network path, using smoltcp or the vendor netif boundary chosen by the bring-up evidence. | Connectivity HIL can reproduce ICMP echo success on real WS63 silicon. |
| C6 Connectivity release | Turn the demo into a repeatable release artifact. | Release notes, known issues, docs, examples, and HIL instructions are aligned; higher-level APIs can be considered after this point. |

## Maintenance Tracks

**HAL 0.6.0 stabilization:** fix stable API blockers only. Keep the default stable surface scoped to HIL-proven WS63 APIs. DMA, embassy, BS2X, and unproven helper surfaces stay behind `unstable` until their invariants and HIL evidence are closed.

**Probe, fwpkg, and toolchain:** make only the changes required to keep connectivity work reproducible. `hisi-fwpkg` remains the image-format fact source; probe-rs should stay a generic transport/debug path and avoid HiSilicon image-format parsing.

**Docs and CI:** preserve the happy path, stable API reference, and connectivity roadmap. Avoid expanding component documentation unless it prevents drift or directly supports C1-C6.

**BSP and board-manager:** deferred. Revisit after real connectivity works and after external boards or user projects create enough pressure for board manifests or board-selection tooling.

## Historical Context

The original roadmap was created from the 2026-05 architecture review and grew into a remediation ledger covering build integrity, HIL bring-up, runtime architecture, HAL API tightening, RF porting, docs, release, and tooling work. That record is archived at [`docs/archive/roadmap-2026-05-2026-07-remediation.md`](docs/archive/roadmap-2026-05-2026-07-remediation.md).

Review ledgers remain under [`docs/review/`](docs/review/). They preserve the evidence trail, but they do not override the current priority order above.
