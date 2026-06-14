//! Build script for the WS63 on-target HIL test crate.
//!
//! Two linker scripts must be on the link line for an embedded-test ELF:
//!
//!   * `ws63-link.x` — hisi-riscv-rt's entry script (startup placement, memory
//!     map, device vectors, and — because we enable the `boot-header` feature —
//!     the 0x300 HiSilicon image header so the bare ELF is bootable). A library
//!     dependency's `rustc-link-arg` does NOT propagate to a downstream
//!     binary/test, so the test binary must request `-Tws63-link.x` itself;
//!     hisi-riscv-rt exports its OUT_DIR on the (propagating) link-search path
//!     so the `-T` resolves (same pattern as examples/ws63/*/build.rs).
//!
//!   * `embedded-test.x` — embedded-test's fragment that keeps the
//!     EMBEDDED_TEST_VERSION marker + the `.embedded_test` test-case section and
//!     redirects the linker-script-presence guard symbol. Provided on the
//!     link-search path by the `embedded-test-linker-script` crate.
fn main() {
    println!("cargo:rustc-link-arg=-Tws63-link.x");
    println!("cargo:rustc-link-arg=-Tembedded-test.x");
    println!("cargo:rerun-if-changed=build.rs");
}
