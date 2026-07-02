# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed (silicon bring-up)

- **TIMER** + **M_DMA** now pass end-to-end on real WS63 silicon (the last two
  `#[ignore]`'d HIL tests). TIMER `current_value()` does the `cnt_req`/`cnt_lock`
  latch handshake; M_DMA `configure_channel()` starts via `dmac_en_chns` and
  detects completion by its auto-clear (vendor-correct, not the QEMU path). Also:
  a new `cache` module for the non-coherent D-cache, and `enable_controller()`
  bypasses the M_DMA auto-clock-gate.
- **UART boot clock** resolved: flashboot's console runs on the raw TCXO (confirmed
  **40 MHz** on this board), not the 160 MHz PLL — use `uart::Config { clock:
  UartClock::Boot, .. }` backed by `soc::chip::uart_boot_clock_hz()`. Two real driver
  bugs (`wdt` saturate-before-narrow, `sfc` floor-before-mask) found + fixed by new
  property tests.

### Added

- **HIL suite grown to 12 driver tests, all passing on silicon** — added
  `efuse_read_byte0_ok` (eFuse read path), `trng_produces_entropy` (real TRNG
  entropy), and `tsensor_reads_in_range` (on-die temperature), all
  self-contained (no jumpers).

### Changed

- **HAL stable API narrowed for 0.6.0** — default builds now expose only scoped
  HIL/soundness-closed surfaces. Public DMA, interrupt/waker async helpers,
  `embassy`, software reset, and other unproven knobs require `unstable`. Removed
  the no-op GPIO `OutputConfig::open_drain`, replaced raw UART `clock_hz` with
  typed `UartClock`, reject invalid I2C 7-bit addresses, and make PWM duty writes
  fallible for out-of-range duty.
- **ws63-pac**: `TIMER%s_CONTROL` gains the `cnt_req`/`cnt_lock` fields and its
  `mode` enum is corrected to the vendor values (`OneShot=0/Periodic=1/FreeRun=3`),
  regenerated from the SVD. (The DMA block was already silicon-correct.) The SVD
  `regen.sh` also now resolves the ws63-pac crate in both the sibling and nested
  (submodule) layouts.
- Host test coverage expanded to **302** unit + property tests (from 82). A code
  review removed 4 tautological `tcxo` status-bit tests (they asserted literals
  against themselves); the `tcxo` driver bit values are now named consts the
  property tests bind to, so a driver-bit change actually fails a test.

### Tracking

- QEMU model divergences from silicon filed as hisi-riscv-qemu **#5** (M_DMA
  `en_chns`), **#6** (TIMER latch handshake), **#7** (SDMA unprovisioned). QEMU is
  not treated as a reference — these are fixed in QEMU, not worked around in Rust.

---

## [2026-06-14] — crates.io: hisi-riscv-hal 0.3.2 · hisi-riscv-rt 0.2.1

### Added

- **Hardware bring-up (validated on real WS63 silicon)** — `blinky` boots and
  blinks GPIO; the full boot chain build → `hisi-fwpkg image` (or link-time
  `boot-header` + `hisi-fwpkg patch-hash`) → `probe-rs download` → boot works;
  semihosting works on target.
- **HIL** — hardware-validated probe-rs flash flow; `hil/cargo-run-hw.sh` (cargo
  runner) + `hil/embedded-test-runner.sh`; on-target `embedded-test` HIL suites
  (run via `cargo test` + probe-rs + semihosting): `tests-hil/tests/hil.rs` — 3
  cross-cutting CPU/PAC tests (`cpu_m_f_csr_invariants`,
  `pac_peripheral_base_addresses`, `pac_peripheral_base_addresses_extra`);
  `crates/hisi-riscv-hal/tests/hil.rs` — 9 driver tests, **all 9 passing on
  silicon** (incl. `timer_counter_advances` and `dma_mem_to_mem` after the
  TIMER-latch and M_DMA `en_chns` silicon fixes — see Unreleased).

### Changed

- Submodule bumps: `hisi-riscv-hal` 0.3.2 (uart `div_fra` fix), `hisi-riscv-rt`
  0.2.1 (`boot-header` feature).

### Fixed

- **HIL** — fixed `hil/hil-smoke.sh` reset_demo marker.

### Docs

- Full mdBook handbook under `docs/` organized by the Diátaxis framework
  (tutorials [app-developer + ecosystem-contributor tracks] / how-to / reference /
  explanation + the 10 component deep-dives), deployed to GitHub Pages (handbook
  at `/`, rustdoc API at `/api/`); the old `docs/architecture/` moved into the
  book.

---

## [2026-06-11] — crates.io: ws63-pac 0.1.3 · bs2x-pac 0.1.0 · hisi-riscv-rt 0.2.0 · hisi-riscv-hal 0.3.0

First crates.io release of the library stack (published via CI in dependency
order). Per-crate changelogs: [`hisi-riscv-hal`](crates/hisi-riscv-hal/CHANGELOG.md) ·
[`hisi-riscv-rt`](crates/hisi-riscv-rt/CHANGELOG.md).

### Added

- **Multi-chip support (BS2X/BS21)** — `bs2x-pac` (BS21/BS20 Peripheral Access Crate) published alongside `ws63-pac`; `hisi-riscv-hal` features `chip-ws63` (default) + `chip-bs21`; full functional peripheral coverage on QEMU (`-M bs21/-M bs20/-M bs22`): SPI, GADC, I2C, KEYSCAN, QDEC, RTC, TRNG, WDT, DMA, PDM real audio, USB real enumeration + vendor LiteOS boot. BS21 milestone M1 achieved: `blinky` + `uart_hello` boot end-to-end on `-M bs21`.
- **New examples** — `semihost_selftest` (semihosting integration), `custom_memory` (custom linker memory layout), async variants (`async_delay`, `embassy_multitask`, `embassy_async_io`, `async_bus`), `spi_loopback` + `i2c_scan` (peripheral validation), `net_ping` (QEMU connectivity base M3), BS2X examples (`spi_loopback`, `gadc_read`, `i2c_scan` for BS21/BS20, `hid_demo`, `pwm_wdt`, `dma_mem`, `clock_rng`).
- **Monorepo restructuring** — reorganized into `crates/{pac/{ws63-pac,bs2x-pac}, hisi-riscv-hal, hisi-riscv-rt}`, `examples/{ws63, bs21, bs20}` (isolated workspaces), `chips/{ws63/{guide,rf,flashboot}, bs2x/guide}`; adjusted default-members and CI for new layout.
- **HIL bring-up scaffold** — `hil/flash.sh` + `hil/hil-smoke.sh` + bring-up checklist for hardware-in-the-loop testing (link-script validation + on-silicon clock/UART bring-up).
- **Async/Embassy infrastructure** — async HAL drivers + embassy time-driven executor integration (6 capstone examples) exercised on QEMU; documented in `docs/architecture/async-embassy.md`.
- **Probe-rs debug support** — fork `hispark-rs/probe-rs` branch `add-hisilicon-ws63-bs21` implements RISC-V-DM-behind-CoreSight via mem-AP DTM, HiSilicon vendor DebugSequence, and flash-algorithm crate (software-complete, pending on-silicon validation).
- **Organization migration** — all repos moved to `github.com/hispark-rs` org; submodule URLs and CI/CD pipelines updated accordingly.
- **Toolchain completion (v1.96.0-1)** — sysroot now includes `rust-analyzer-proc-macro-srv`, incremental-build `cargo`, `rust-gdb`/`rust-lldb` + GDB pretty-printers for `gdb-multiarch` QEMU debugging, `rust-src`, `llvm-tools`; hardened release CICD.

### Changed

- **Test alignment** — BS2X full peripheral coverage exercised on QEMU for functional validation (SPI, GADC, I2C, KEYSCAN, QDEC, RTC, TRNG, WDT, DMA, PDM, USB); vendor C SDK (loaderboot → flashboot → LiteOS) boots on `-M bs21`.
- **Documentation** — overview.md and architecture docs extended with BS2X coverage; ROADMAP aligned with Phase 7 (HAL polish + publish) in progress.
- **CI/CD** — issue-handler tightened to title-only whole-word matching; CI adjusted for new monorepo layout.

### Notes

- BS2X connectivity (BLE/SLE) feasibility: radio-MMIO emulation is a measured dead end (B_CTL 0x59000000 = 56 write-only PHY regs + IRQ-26 PHY-event wall, pure blob); HCI boundary is blob-on-blob; full writeup in `docs/bs21-connectivity-feasibility.md`.
- Wi-Fi connectivity (ws63-rf-rs): porting layer complete (scheduler/OSAL/data-path/timer/netif→smoltcp); blob TX/RX + on-silicon validation deferred (ROADMAP Phase 4/5).
- GitHub issues tracking open tasks: `hispark-rs/hisi-riscv-rs` #6–#21 + `hispark-rs/probe-rs` #1.

---

## [0.2.1] - 2026-06-02

### Changed

- **Releases are now owned by each crate's own repo.** The monorepo tag only cuts the firmware GitHub Release; crate publishing moved to each submodule's own `release.yml` (pac/rt/hal), triggered by a `v*` tag in that repo with its own `CRATES_IO_TOKEN`. Removed the parent's `publish` job.
- Added `hisi-riscv-rt`'s own release workflow (pac/hal already had theirs).

### Notes

- First releases via the per-repo pipelines: `ws63-pac 0.1.3`, `hisi-riscv-rt 0.1.1`, `hisi-riscv-hal 0.2.1` (each published by its own repo).

## [0.2.0] - 2026-06-02

### Added

- **ws63-rf-rs porting layer** — complete cooperative scheduler backing OSAL contract (scheduler/runtime internal)
- **OSAL shims** — 33 osal_adapt_* symbols + real timed blocking, full spinlock/atomic/queue/event/vmalloc/str/time implementations
- **Condvar + libc** — osal_wait implementation with oal/uapi leaf symbols
- **Data path** — real FRW worker thread + HCC transport
- **Software timer service** — frw_dmac_timer_* / osal_adapt_timer_* real implementations
- **netif→smoltcp bridge** (feature `net`) — frame round-trip connectivity for Wi-Fi MAC link
- **netif/litos seam** — full MAC link achieving Wi-Fi-init symbol closure
- **Log event** — log_event_wifi_print3 support; Wi-Fi library vendoring (open-network MVP)
- **DMA enhancements** — SDMA 8-11 mapping + peripheral-DMA validation
- **CI/CD pipeline** (7 workflows):
  - `ci.yml` — build check, clippy, rustfmt, workspace build, host tests, security audit
  - `ci-nightly.yml` — nightly Rust builds, nightly clippy, binary size reporting
  - `documentation.yml` — API docs build, GitHub Pages deploy, link checking
  - `issue-handler.yml` — auto-labeling new issues, welcome-first-time-contributors
  - `merge-conflict.yml` — conflict marker detection, PR labeling
  - `release.yml` — GitHub Release on tag push, crates.io publishing
  - `dependabot.yml` — weekly Cargo + monthly GitHub Actions updates
- **Dependabot** configuration for automated dependency updates
- **Changelog** configuration (`.github/changelog-config.json`)
- **CLAUDE.md** — agent guidance for working in this repo
- **Run skill** (`.claude/skills/run-ws63-rs/`) — build driver script + SKILL.md
- **hisi-riscv-hal** expanded from 10 to 31 source files, covering 35 PAC peripherals
- **hisi-riscv-hal** now implements 25+ embedded-hal / embedded-hal-nb / embedded-io traits
- **hisi-riscv-hal** aligned with esp-hal patterns (RAII clock guards, type-state GPIO, sealed traits)
- **Phase-3 Wi-Fi blob link spike** (wifi_blob_link example) + ROADMAP documentation
- **Host unit tests** — genuine host unit tests (Phase 2)
- **Trap handling** — vectored mtvec + explicit trap-table layout + unified trap stacks

### Changed

- **Directory structure** — ws63-RF nested under ws63-rf-rs (prevent lateral deps)
- **Directory structure** — ws63-svd nested under ws63-pac (generation source owned by its consumer)
- **Scheduler/runtime** — made internal (not a public API)
- hisi-riscv-hal submodule updated through 5 feature merges
- `cargo fmt` applied workspace-wide
- Clock control refactored: duplicated register dispatch eliminated
- GPIO module: new `Input`/`Output`/`Flex` drivers alongside legacy `GpioPin<MODE>`

### Fixed

- **DMA wiring** — correct request IDs + wiring; fix hisi-riscv-hal/hisi-riscv-rt standalone CI
- **Flashboot** — correct image-header layout + honest A/B verification (Phase 2)
- **CI** — Docs fix: unlink private apply_pull
- **CI** — standalone CI: drop pinned lock + fix doc link
- **GPIO** — dead-code cleanup + pull/trigger support (Phase 2)

### Removed

- libwpa_supplicant.a (moved to open-network MVP)

---

## [0.1.0]

### Added

- **CI/CD pipeline** (7 workflows):
  - `ci.yml` — build check, clippy, rustfmt, workspace build, host tests, security audit
  - `ci-nightly.yml` — nightly Rust builds, nightly clippy, binary size reporting
  - `documentation.yml` — API docs build, GitHub Pages deploy, link checking
  - `issue-handler.yml` — auto-labeling new issues, welcome-first-time-contributors
  - `merge-conflict.yml` — conflict marker detection, PR labeling
  - `release.yml` — GitHub Release on tag push, crates.io publishing
  - `dependabot.yml` — weekly Cargo + monthly GitHub Actions updates
- **Dependabot** configuration for automated dependency updates
- **Changelog** configuration (`.github/changelog-config.json`)
- **CLAUDE.md** — agent guidance for working in this repo
- **Run skill** (`.claude/skills/run-ws63-rs/`) — build driver script + SKILL.md
- **hisi-riscv-hal** expanded from 10 to 31 source files, covering 35 PAC peripherals
- **hisi-riscv-hal** now implements 25+ embedded-hal / embedded-hal-nb / embedded-io traits
- **hisi-riscv-hal** aligned with esp-hal patterns (RAII clock guards, type-state GPIO, sealed traits)

### Changed

- hisi-riscv-hal submodule updated through 5 feature merges
- `cargo fmt` applied workspace-wide
- Clock control refactored: duplicated register dispatch eliminated
- GPIO module: new `Input`/`Output`/`Flex` drivers alongside legacy `GpioPin<MODE>`
