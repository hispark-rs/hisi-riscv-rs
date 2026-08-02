# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Release train anchor: `hisi-hal 0.7.0-alpha.6`.

### Changed

- **A5B worker admission** — publish `hisi-rf-ws63 0.1.0-alpha.64` and
  `hisi-rf 0.1.0-alpha.74`, separating the seven-slot vendor bootstrap from
  the optional eighth Rust worker slot. A credential-free WS63 init/scan HIL
  now reaches the bounded native runner under the r8 resource profile.
- **HAL package migration** — renamed the active HAL package, Rust crate, GitHub
  repository, and parent submodule path from `hisi-riscv-hal` /
  `hisi_riscv_hal` / `crates/hisi-riscv-hal` to `hisi-hal` / `hisi_hal` /
  `crates/hisi-hal`. The old `0.6.x` package remains on its maintenance branch.
- **Consumer closure** — migrated WS63 and BS2X examples, RF, HIL runners,
  template `v0.7.0-alpha.3`, CI, skills, chip metadata, mdBook, and multi-chip
  rustdoc publishing to the new package without relying on GitHub redirects.
- **Single-PAC dependency closure** — published `ws63-pac 0.4.0`,
  `hisi-riscv-rt 0.5.5`, and `hisi-hal 0.7.0-alpha.3`; runtime, HAL, examples,
  HIL, and generated projects now resolve the same corrected PAC major.
- **Async IRQ ownership** — HAL async drivers expose ISR hooks without exporting
  strong device handlers. Firmware or RTOS trap routing owns every vector, so
  Cargo feature unification cannot inject handlers into unrelated binaries.
- **WPA handshake hardware crypto** — move SHA/HMAC and AES block operations to
  the token-owned WS63 SPACC/KM backend. Upstream hostap key unwrap and CMAC keep
  their protocol implementation while using fallible hardware capabilities; no
  hardware error silently falls back to RustCrypto.
- **Crypto release units** — publish `hisi-crypto 0.1.0-alpha.4` and
  `hisi-crypto-ws63 0.1.0-alpha.2` with the P-256, SPACC hash/HMAC/AES, and
  explicit static-storage contracts used by the current connectivity path.
- **RTOS compatibility contract** — publish `hisi-rf-rtos-driver
  0.1.0-alpha.13` with sixteen executable scheduling/wait/context scenarios and
  `hisi-rtos 0.1.0-alpha.7` with priority-ordered semaphore grants,
  fail-closed resource destruction, and invalid-context parity between the
  production scheduler and deterministic backend. WS63-specific priority, tick
  and return-code conversion lives in its archive-bound machine profile.
- **Cross-host RF artifacts** — validate Cargo-delivered normalized WPA2/WPA3
  target archives on native Linux, macOS, and Windows without the vendor source
  submodule. The matrix also exercises target paths containing spaces and
  non-ASCII text; final single-dependency application linking remains an A5F gate.
- **Release preflight** — standalone crate packaging now runs full
  `cargo package --locked` verification; `--no-verify` no longer allows a
  publish-only compile failure to escape local preflight.
- **Incremental RF observability** — publish `hisi-rf-ws63 0.1.0-alpha.19`
  and `hisi-rf 0.1.0-alpha.30` with secret-free, stage-level
  bootstrap metrics exposed through the safe facade. The default backend and
  blocking vendor initialization semantics remain unchanged.
- **Actionable RF diagnostics** — publish `hisi-rf-ws63 0.1.0-alpha.31` and
  `hisi-rf 0.1.0-alpha.41` with credential-free RV32/QEMU/WS63 parity for
  production association-rejection, first-EAPOL-timeout, cancellation and
  backend-timeout paths. Cancellation and timeout now drive the real
  incremental operation state machine and prove terminal-slot recovery rather
  than only serializing preconstructed errors.
- **Incremental resource conservation** — add a production-path adversarial
  cancellation regression covering key release, bounded pending ownership,
  timer cleanup, late-success suppression, and generation-safe slot reuse.
- **A5 evidence reconciliation** — close the stale work-budget and RTOS
  conformance checklist entries against their existing transition-profile HIL,
  shared scenario, negative-test, Kani, TLA+, and real-silicon evidence while
  retaining the pure-WPA3 external gate.
- **Self-contained Wi-Fi template release** — publish `hisi-rs-template
  v0.7.0-alpha.11`; generated Wi-Fi projects pin `hisi-rf 0.1.0-alpha.48`,
  `hisi-rtos 0.1.0-alpha.14` and the official nightly locally, carry their own
  target/linker and root release-profile configuration, and emit the ELF,
  FlashPlan image, plan JSON and deterministic profile resource JSON together.
  Template tag pushes now rerun the full generated-project and native
  Linux/macOS/Windows resource-report matrices before creating a GitHub
  prerelease.
- **RF facade migration contract** — add the application migration guide from
  `ws63-rf-rs` to the single `hisi-rf` dependency, including profile, storage,
  composition-root, RTOS and dependency-tree mappings. The old crate remains a
  bounded maintainer oracle until its pure-WPA3 parity and migration-release
  removal gates are satisfied.
- **Incremental release closure** — publish `hisi-rf-core
  0.1.0-alpha.15`, `hisi-rf-ws63 0.1.0-alpha.32`, and `hisi-rf
  0.1.0-alpha.42`. The core preserves local continuation when a deadline adds a
  timer subscription, and the released consumer fixture now pins and verifies
  the exact facade/core/backend closure in clean and offline builds.
- **A5 evidence correction** — reopen bounded `start`/`cancel`, real key-hook
  conservation, opaque facade/runtime selection, and strict QEMU/HIL gates
  exposed by adversarial review. Pure WPA3 remains a separate externally
  blocked gate rather than the only unfinished A5 item.
- **Typed timeout and cancellation contract** — publish `hisi-rf-core
  0.1.0-alpha.17`, `hisi-rf-ws63 0.1.0-alpha.44`, and `hisi-rf
  0.1.0-alpha.54`. Protocol operations, backend lifecycle waits, and
  application wait deadlines now use separate types and diagnostics; dropping
  an accepted operation future requests production cancellation while vendor
  cleanup remains runner-owned.
- **Centralized Wi-Fi example configuration** — move profile timeouts, runner
  budget, scan capacity, and compile-time credentials into each application's
  configuration module. Template `v0.7.0-alpha.15` generates the same contract
  and emits a separate, secret-free application-wait diagnostic instead of
  relabeling an outer deadline as a backend failure.
- **Protocol-based connectivity gate** — replace public ICMP as a hard HIL gate
  with DHCP, direct gateway ARP, and a validated UDP DNS response. Public ICMP
  remains an observational loss metric and no longer turns a working local
  data path into a firmware failure.
- **Native WS63 RTOS startup** — publish `hisi-rtos 0.1.0-alpha.15` with a
  typed `hisi_rtos::ws63::start` facade that owns TIMER/SWI wiring, the 24 MHz
  scheduler clock, interrupt handlers, and global-interrupt startup. WS63
  connectivity examples and template `v0.7.0-alpha.18` no longer duplicate
  those chip-port mechanisms.

### Verification

- HAL rename API parity, stable API snapshot, register-access policy, 315 host
  tests, WS63/BS21 feature builds, WS63/BS21 rustdoc, and stable/unstable HIL
  ELF linking pass under the new package name.
- Template CI generates and links WS63 blinky, WS63 `uart_hello`, and BS21
  blinky from crates.io; both WS63 projects also generate FlashPlan images.
- SPACC AES standard vectors and recovery checks pass on silicon; an unchanged
  upstream WPA2 image completed 20/20 nRST associations with 720/720 AES block
  operations and no hardware failures.
- WS63 RF bootstrap diagnostics pass minimal-target, WPA2/WPA3 blocking and
  incremental builds, independent packaging, and Linux/macOS/Windows final
  linking; the facade additionally passes six crates.io-only consumer lanes,
  offline read-only registry rebuilds, and concurrent profile builds.
- The same typed-error fixture emits identical PMF status-30 and EAPOL-timeout
  diagnostics in QEMU and on WS63 after a 3 MHz fully verified download.
- The operation-level fixture emits identical cancellation and scan-timeout
  diagnostics in QEMU and on WS63, then starts a new operation to prove slot
  recovery; the 3 MHz fully verified download completed in 2.38 seconds.
- Template branch/tag CI runs `30376540180` and `30377443816` generate the
  complete WS63 Wi-Fi project, build its release image and run the
  resource-report helper on native Linux, macOS and Windows. The successful tag
  run published `v0.7.0-alpha.11` as a non-draft prerelease.
- RTOS CI run `30492759785` passed host, RV32 WS63/Embassy, Kani, and TLA+
  checks; publish run `30492943437` released alpha.15. Template CI run
  `30493223527` generated and built the new Wi-Fi starter and passed native
  Linux/macOS/Windows resource-report jobs.
- The RTOS-facade connectivity ELF passed full 1 MHz readback verification and
  real-silicon init, scan, WPA2 connect, DHCP, direct gateway ARP, UDP DNS, and
  lease renewal. A preceding 3 MHz first-page timeout was classified separately
  as probe transport failure and recovered with the official full FWPKG.

---

## [2026-07-12] — WS63 connectivity baseline · HAL 0.6.0

Release train anchor: `hisi-riscv-hal 0.6.0`. This snapshot freezes the
HAL 0.6 stable surface and the first reproducible WS63 connectivity baseline:
Wi-Fi init, scan, WPA2-Personal association, DHCP/ARP, and ICMP ping on real
silicon.

### Added

- **WS63 connectivity C1-C5** — link and boot the vendor Wi-Fi runtime, initialize
  RF, scan, associate to open and WPA2-Personal networks, obtain a DHCP lease,
  resolve ARP, and ping through the Rust-visible L2 path. The UART markers and
  final image/layout evidence are frozen as the A0 migration baseline.
- **WPA2-Personal closure** — crop the vendor supplicant to the required source
  profile, derive PMKs through the WS63 public crypto UAPI plus RustCrypto
  primitives, and remove the supplicant's direct mbedTLS archive dependency.
  Future application TLS remains a separate `hisi-tls` layer whose default
  backend is mbedTLS.
- **HAL stable API freeze** — commit a generated public-API snapshot, add
  stable-only rustdoc checks, and run the official `riscv32imfc` target in the
  standalone HAL CI instead of accepting host-only builds.
- **Connectivity execution plans** — record the RF5/A0-R0 decomposition,
  component ownership, future `hisi-crypto`/`hisi-tls` boundaries, and the
  post-0.6 `hisi-riscv-hal` to `hisi-hal` migration.

### Changed

- **`hisi-riscv-hal 0.6.0` published** — freezes the default WS63 stable
  surface. The self-contained real-silicon suite passes 30/30 by default and
  32/32 with `unstable`; DMA, RF helpers, Embassy, and BS2X remain experimental.
- **`ws63-pac 0.2.2` published** — adds the shared-RAM, RF power, factory XO-trim,
  and mask-ROM patch-controller definitions required by standalone HAL and RF
  builds.
- **`hisi-riscv-rt 0.5.3` published** — publishes the RF memory/ROM-patch startup
  support and adds real WS63/BS21 link-smoke CI. WS63-only Wi-Fi ROM-data
  relocation is excluded from the BS2X compatibility adapter.
- **`hisi-rs-template v0.6.0`** — generated WS63/BS21 projects consume the
  published HAL 0.6.0, RT 0.5.3, and PAC releases; its three-project build/image
  matrix runs without parent path patches.
- **`hisi-fwpkg 0.3.2` published** — all ELF/headered-image/FWPKG planning paths
  now materialize linker-aligned verified tails as erased `0xFF`, so image,
  hash, erase, and write ranges describe the same bytes flashboot verifies.
- **RF linker contract** — validate the Rust-linked ELF against the vendor oracle
  map and fail before flashing when relocation/layout addresses drift.

### Fixed

- Align the WS63 I2C HAL with the vendor v150 non-FIFO polling sequence and add
  silicon evidence for `DONE` followed by `ACK_ERR`; configure the documented
  EVB I2C pads in the `i2c_scan` example.
- Make `hisi-fwpkg patch-hash` hash linker-aligned trailing bytes as erased
  flash, matching the canonical FlashPlan and restoring embedded-test images
  whose verified body ends on a padded boundary.
- Close RF runtime mismatches in ROM-owned state, timed waits, task CPU context,
  TCXO selection, calibration eFuse access, zero-copy pbuf headroom, factory MAC
  loading, and the vendor lwIP data-plane ABI.
- Make connectivity HIL package the canonical planned image and pulse the J-Link
  nRST line before UART capture, eliminating stale image/reset behavior.
- Align the RT ROM-patch pointer code and `rf_port_demo` with the pinned
  nightly's Clippy rules and current packed Wi-Fi log ABI; keep the port-only
  demo independent of vendor archives now owned by the blob/init examples.

---

## [2026-07-09] — official nightly migration · docs happy path · HAL alpha

Release train anchor: `hisi-riscv-hal 0.6.0-alpha.2`. The parent repository
version for this train represents the ecosystem snapshot around that HAL
pre-release: submodule pointers, docs, template contracts, toolchain policy,
image tooling, and HIL/smoke workflows that make the HAL release usable together.

### Added

- **Official Rust toolchain path** — the ecosystem now builds on upstream
  `nightly-2026-07-09` with the built-in `riscv32imfc-unknown-none-elf` target and
  `-Zbuild-std=core,alloc`. The old custom `hisi-riscv` tarball path is no longer
  the happy path; `hisi-riscv-rust-toolchain` is repurposed as an upstream nightly
  radar for target/Tier-2 readiness.
- **Executable tutorial contracts** — `scripts/tutorial-contracts.sh` is now the
  single source for tutorial command snippets. mdBook snippets, docs drift checks,
  template generation smoke, example builds, and `hisi-fwpkg plan --image-output`
  all use the same contract.
- **Chip/version-aware documentation** — the mdBook handbook has chip/version
  selectors and chip-aware snippets backed by shared metadata. Rustdoc publishing
  is arranged by chip/version (`/api/<version>/<chip>/...`) so docs link to the
  correct feature-specific API surface.
- **`hisi-rs-template` submodule** — app templates are versioned and tested with
  the parent happy path, using crates.io dependencies in generated projects and
  the parent workspace only as a development override.

### Changed

- **HAL `0.6.0-alpha.1` published** — first 0.6 stabilization pre-release with
  stable/unstable gating, typed-config tightening, register-access cleanup, and
  expanded HIL evidence. The stable surface is now scoped to HIL-proven WS63 APIs;
  BS2X and unproven helpers require `unstable`.
- **HAL `0.6.0-alpha.2` published** — small cleanup release on top of alpha.1:
  rustdoc private-link cleanup plus warning-free default stable builds.
- **Register truth source tightened** — `ws63-pac 0.2.1` and `bs2x-pac 0.1.2`
  regenerate from audited SVDs; HAL register access is routed through PAC fields
  rather than raw MMIO where the SVD can express the register semantics.
- **Probe-rs branch references** now point to the HIL baseline branch
  `add-hisilicon-ws63-bs21-hil-baseline`, the fork carrying the current WS63 target
  YAML and flash algorithm baseline.

### Fixed

- **Image semantics drift** — smoke/download paths now route through
  `hisi-fwpkg plan --image-output`; probe-rs uses generic
  `download --binary-format bin --base-address <plan.base_addr>` and no longer
  owns HiSilicon header/hash/body-range rules.
- **Docs deployment** — versioned mdBook/rustdoc publishing and selector layout
  fixes keep `/main/`, `/latest/`, released versions, and chip API links coherent.
- **Flash algorithm handoff** — the WS63 flash algorithm now resets the SPI-NOR
  state after programming and restores status registers in the form flashboot
  expects; probe-rs target YAML is synced to that algorithm blob.

## [2026-07-05] — runtime adapter architecture

### Changed

- **`hisi-riscv-rt 0.5.0` / `0.5.1`** — runtime split into CPU-generic core plus
  chip adapters. WS63 owns its startup/linker/cache/PMP/boot-header resources;
  BS2X has its own adapter and default `memory.x`; Hi3322 is documented as a future
  porting target rather than guessed into WS63 startup.
- **Canonical linker script** is now `hisi-riscv-link.x`; the deprecated
  `ws63-link.x` alias was removed after consumers migrated.
- **PAC-owned interrupt symbols** — WS63 and BS2X interrupt symbol generation comes
  from the corresponding PAC `rt` feature instead of in-tree runtime copies.
- **Runtime stable/unstable gates** mirror HAL policy: WS63 adapter is stable,
  BS2X and experimental startup paths require `unstable`.

### Added

- **Experimental `riscv-rt-start-experiment` path** — delegates `.data`/`.bss`/FPU
  setup to `riscv-rt::_start`, while the WS63 adapter handles trap dispatch,
  cache/PMP relocation, and boot header. Verified on real WS63 with `uart_hello`.
- **Hi3322 runtime porting spec** — documents TES/TEE reset/vector facts from the
  vendor tree and records where `riscv-rt` can be reused versus where a custom
  reset/vector path may be required.

## [2026-06-30] — crates.io: hisi-riscv-hal 0.5.1

### Added

- **Peripheral-paced DMA for SPI** — typed DMA channel tokens, mem-to-peripheral /
  peripheral-to-mem transfers, `Spi::with_dma`, `SpiDma::write_dma`, and
  full-duplex `transfer_dma`, with SPI0 loopback HIL coverage on real WS63 silicon.
- **`UartDma` ergonomic API** — register sequence and ownership model added, with
  data-correctness deferred behind the UART1 board/pad routing issue.

### Fixed

- **DMA quiescence on timeout/drop** — `Transfer::wait` and `Drop` now halt, wait
  for `active` to drain, and disable the channel before returning buffers, avoiding
  a latent use-after-free if a channel wedges.

## [2026-06-16] — crates.io: hisi-riscv-hal 0.5.0 · hisi-riscv-rt 0.4.0

### Changed

- **Typed-config pass across HAL drivers** — writable config values are now values
  silicon can actually run: typed SPI frequency/data bits, I2C speeds, UART baud,
  I2S role configs, WDT timeouts, timer durations, LSADC counts, and SFC data sizes.
- **Drop-to-disable semantics** — watchdog, PWM, and GPIO output handles return
  hardware to safe states on drop, with explicit escape hatches for intentionally
  armed/running/latching peripherals.
- **HAL chip feature de-default** — standalone HAL builds require an explicit chip
  feature; the parent workspace still builds through example feature unification.
- **RT direct-mode interrupt routing** — `hisi-riscv-rt 0.4.0` dispatches custom
  IRQs through PAC `device.x`-named weak symbols, allowing driver-owned handlers
  without app-side `mcause` shims.

### Added

- **Expanded HIL coverage** — timer latch, GPIO/IRQ routing, DMA, I2C, PWM, WDT,
  TSENSOR/TRNG/eFuse and other driver tests ran on real WS63 silicon during the
  0.5 stabilization pass.

## [2026-06-15] — crates.io: ws63-pac 0.2.0 · hisi-riscv-rt 0.2.2 · hisi-riscv-hal 0.4.0

### Fixed

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
- **BS2X target marked experimental** — `chip-bs21` now requires `unstable`; BS20/BS21
  examples and HIL feature forwarding opt in explicitly. The stable API promise is
  scoped to the WS63 HIL-proven subset until BS2X silicon HIL exists.
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
