//! Build script for the on-target HIL test crate (chip-gated: WS63 default,
//! BS21 via `--features chip-bs21`).
//!
//! Two linker scripts must be on the link line for an embedded-test ELF:
//!
//!   * `ws63-link.x` — hisi-riscv-rt's entry script (startup placement, memory
//!     map, device vectors, and — under WS63's `boot-header` feature — the 0x300
//!     HiSilicon image header so the bare ELF is bootable). The entry-script
//!     *name* is `ws63-link.x` for BOTH chips: hisi-riscv-rt's build.rs always
//!     writes `ws63-link.x` (it only varies what it `INCLUDE`s — WS63 device.x
//!     vs the PAC-supplied BS21 device.x, and the boot-header only when its
//!     feature is on). So this single `-Tws63-link.x` flag is chip-agnostic and
//!     is emitted unconditionally; no per-`CARGO_FEATURE_CHIP_*` gate is needed.
//!     A library dependency's `rustc-link-arg` does NOT propagate to a
//!     downstream binary/test, so the test binary must request the `-T` itself;
//!     hisi-riscv-rt exports its OUT_DIR on the (propagating) link-search path
//!     so it resolves (same pattern as examples/ws63/*/build.rs and
//!     examples/bs21/*/build.rs, which both use `-Tws63-link.x`).
//!
//!   * `embedded-test.x` — embedded-test's fragment that keeps the
//!     EMBEDDED_TEST_VERSION marker + the `.embedded_test` test-case section and
//!     redirects the linker-script-presence guard symbol. Provided on the
//!     link-search path by the `embedded-test-linker-script` crate. Used for
//!     both chips.
//!
//! NOTE: under `chip-bs21` there is no bundled WS63 memory.x (rt's
//! `bundled-memory-x` is off when default-features are dropped), and this crate
//! does not yet ship a BS21 `memory.x`, so `ws63-link.x`'s `INCLUDE memory.x`
//! will not resolve for BS21 until a BS21 board + memory.x are added here (see
//! examples/bs21/clock_rng for the pattern).
fn main() {
    // Entry script name is the same (`ws63-link.x`) for WS63 and BS21 — rt only
    // varies the INCLUDEd fragments, not the entry-script name — so no chip gate.
    println!("cargo:rustc-link-arg=-Tws63-link.x");
    println!("cargo:rustc-link-arg=-Tembedded-test.x");
    println!("cargo:rerun-if-changed=build.rs");
}
