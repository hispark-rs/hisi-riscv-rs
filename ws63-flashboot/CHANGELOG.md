# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial Rust second-stage bootloader implementation (port from fbb_ws63 C code)
- UART debug logging module for boot-time diagnostics
- SHA256 pure-Rust implementation for image integrity verification
- SFC (SPI Flash Controller) module with quad-SPI support
- Image header parsing and validation (ImageHeader, KeyArea, CodeInfo structs)
- Assembly startup code (startup.S) with CPU init (PMP, mtvec, stack, FPU, BSS zeroing)
- Boot sequence: TCXO detect → flash clock PLL → UART PLL → WDT init → SFC init → FAMA remap → image verification → jump to app
- CPU clock adaptation module (boot_clock_adapt) for UART/WDT/timer rate scaling
- eFuse initialization and chip type detection
- Partition table parsing stub and upgrade mode detection
- A/B partition fallback (main → backup on invalid)
- Image integrity verification via SHA256 hash comparison
- Unit tests for image header validation edge cases
- Architecture documentation and design rationale

### Fixed
- SFC: chunked reads to 64-byte commands (fixed multi-word hang and data truncation)
- UART: enabled UART_EN control bit (debug output was previously silent)
- Image verification: now correctly compares against SHA256 hash stored in image header
- Image header layout: corrected KeyArea and CodeInfo field offsets to match vendor format (fbb_ws63 secure_verify_boot.h); code_area_len @CodeInfo+0x24, code_area_hash @+0x28
- A/B partition logic: removed misuse of flashboot self-recovery flag; now single-image boot with honest comment that real A/B is via partition table magic
- Removed unused SFC constants and simplified image validation to single-expression validate()
- Startup.S: proper section ordering with .text.entry first
- Code refactoring: extracted reusable helpers (switch_flash_to_pll, switch_uart_to_pll, wdg_init, wdg_feed, delay, log, halt) reducing main.rs by 46%

### Changed
- Marked crate as EXPERIMENTAL and NOT secure boot (publish=false, not in default-members)
- Image verification reframed as integrity check (NOT authenticity); stub partition table parser documented
- Clarified that production should reuse vendor flashboot with Rust app in APP partition
- Added SAFETY comments to all raw pointer dereferences
- Updated image.rs validation logic to match vendor header format and added tests for edge cases
- Documentation: comprehensive README warning against production use, architecture guide, and ROADMAP roadmap planning future phases

### Removed
- Unused ws63-pac dependency (flashboot uses raw MMIO for autonomy)
- Unused SFC_INT_MASK constant
- Unused ImageHeader::zeroed() method

## Notes

**This crate is EXPERIMENTAL and NOT for production use.** It is a learning-oriented port of the vendor flashboot from C to Rust, demonstrating the WS63 boot sequence. Key limitations:

- Image verification is SHA256 integrity check only (NOT cryptographic signature verification with efuse root key)
- Partition table parsing is a stub (always returns FLASH_START)
- A/B upgrade, FOTA, image decompression, and flash encryption are NOT implemented
- Any party with flash write access can recompute SHA256 and inject arbitrary code

Production deployments should reuse the vendor fbb_ws63 flashboot as second-stage loader and package Rust applications as signed images for the vendor partition table.

Phase 1 (linker-script integration, blinky linking) complete. Phase 2 (header layout and A/B honesty) complete. Later phases (phase 3+) focus on connectivity and full bootloader features.
