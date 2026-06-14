//! On-target HIL test crate for the WS63 RISC-V target.
//!
//! The real tests live in `tests/hil.rs` (a `harness = false` integration test
//! driven by `#[embedded_test::tests]`). This library root exists only so the
//! crate is a normal cargo package; it intentionally carries no runtime code.
#![no_std]
