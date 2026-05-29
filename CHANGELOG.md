# Changelog

All notable changes to ws63-rs will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

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
