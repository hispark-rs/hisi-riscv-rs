# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.1] - 2026-06-02

### Changed

- **Releases are now owned by each crate's own repo.** The monorepo tag only cuts the firmware GitHub Release; crate publishing moved to each submodule's own `release.yml` (pac/rt/hal), triggered by a `v*` tag in that repo with its own `CRATES_IO_TOKEN`. Removed the parent's `publish` job.
- Added `ws63-rt`'s own release workflow (pac/hal already had theirs).

### Notes

- First releases via the per-repo pipelines: `ws63-pac 0.1.3`, `ws63-rt 0.1.1`, `ws63-hal 0.2.1` (each published by its own repo).

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
- **ws63-hal** expanded from 10 to 31 source files, covering 35 PAC peripherals
- **ws63-hal** now implements 25+ embedded-hal / embedded-hal-nb / embedded-io traits
- **ws63-hal** aligned with esp-hal patterns (RAII clock guards, type-state GPIO, sealed traits)
- **Phase-3 Wi-Fi blob link spike** (wifi_blob_link example) + ROADMAP documentation
- **Host unit tests** — genuine host unit tests (Phase 2)
- **Trap handling** — vectored mtvec + explicit trap-table layout + unified trap stacks

### Changed

- **Directory structure** — ws63-RF nested under ws63-rf-rs (prevent lateral deps)
- **Directory structure** — ws63-svd nested under ws63-pac (generation source owned by its consumer)
- **Scheduler/runtime** — made internal (not a public API)
- ws63-hal submodule updated through 5 feature merges
- `cargo fmt` applied workspace-wide
- Clock control refactored: duplicated register dispatch eliminated
- GPIO module: new `Input`/`Output`/`Flex` drivers alongside legacy `GpioPin<MODE>`

### Fixed

- **DMA wiring** — correct request IDs + wiring; fix ws63-hal/ws63-rt standalone CI
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
- **ws63-hal** expanded from 10 to 31 source files, covering 35 PAC peripherals
- **ws63-hal** now implements 25+ embedded-hal / embedded-hal-nb / embedded-io traits
- **ws63-hal** aligned with esp-hal patterns (RAII clock guards, type-state GPIO, sealed traits)

### Changed

- ws63-hal submodule updated through 5 feature merges
- `cargo fmt` applied workspace-wide
- Clock control refactored: duplicated register dispatch eliminated
- GPIO module: new `Input`/`Output`/`Flex` drivers alongside legacy `GpioPin<MODE>`
