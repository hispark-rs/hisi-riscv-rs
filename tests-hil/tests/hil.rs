//! On-target (semihosting) HIL tests for the WS63 RISC-V target.
//!
//! Built with `cargo test -p tests-hil --target riscv32imfc-unknown-none-elf`
//! and run on real silicon by the patched probe-rs fork via the
//! `hil/embedded-test-runner.sh` cargo runner (see ../hil/README.md). Each test
//! is executed in turn over the semihosting channel; the result is reported back
//! to `probe-rs run` (libtest-compatible).
//!
//! ## Entry-point interaction with hisi-riscv-rt
//!
//! We do NOT use `hisi_riscv_rt::entry` here. embedded-test exports the C symbol
//! `main` (its test dispatcher), and hisi-riscv-rt's `runtime_init` (the tail of
//! the assembly startup) calls `extern "Rust" fn main()` after BSS-zero/data-copy
//! — so embedded-test's `main` IS the entry. hisi-riscv-rt still supplies the
//! reset vector, the `critical-section-single-hart` impl (backing
//! portable-atomic's RMW polyfill on this no-atomic core), and — via the
//! `boot-header` feature — the 0x300 image header that makes the ELF bootable.
//! embedded-test also provides the `#[panic_handler]` (it aborts via
//! semihosting), so we must not define one.
//!
//! The tests are self-contained: no jumpers / external wiring, safe on a bare
//! board and under QEMU.

#![no_std]
#![no_main]

// Pull in hisi-riscv-rt so its startup, reset vector, linker scripts and
// critical-section impl are linked even though we never name a symbol from it.
use hisi_riscv_rt as _;

/// Read the low 32 bits of the `mcycle` CSR (Zicsr / Zicntr).
fn rdcycle() -> u32 {
    let c: u32;
    unsafe {
        core::arch::asm!("csrr {0}, mcycle", out(reg) c, options(nomem, nostack));
    }
    c
}

#[cfg(test)]
#[embedded_test::tests]
mod tests {
    use super::rdcycle;
    use core::hint::black_box;
    use ws63_pac as pac;

    /// `#[init]` runs before every test. It takes the singleton `Peripherals`
    /// once and hands them to each test as shared state — proving the PAC's
    /// critical-section-guarded `take()` (backed by hisi-riscv-rt's
    /// single-hart critical-section impl) works on-target.
    #[init]
    fn init() -> pac::Peripherals {
        pac::Peripherals::take().expect("PAC Peripherals::take() returned None on first call")
    }

    /// CPU-only invariants: M-extension multiply, F-extension hard-float (ilp32f)
    /// arithmetic, and the `mcycle` CSR advancing. Mirrors
    /// examples/ws63/semihost_selftest. `black_box` stops the optimiser folding
    /// these away so the real M/F/CSR instructions execute on the core.
    #[test]
    fn cpu_m_f_csr_invariants() {
        // M extension: integer multiply.
        assert_eq!(black_box(123u32) * black_box(456u32), 56_088);

        // F extension (hard-float, ilp32f): single-precision arithmetic.
        let x = black_box(2.0f32);
        assert_eq!(x * x + 1.0, 5.0);

        // Zicsr / Zicntr: mcycle advances across a busy loop.
        let c0 = rdcycle();
        let mut acc = 0u32;
        for i in 0..1000u32 {
            acc = acc.wrapping_add(black_box(i));
        }
        black_box(acc);
        assert_ne!(rdcycle(), c0, "mcycle did not advance");
    }

    /// Structural PAC fact: the compile-time base-address constants of a few
    /// WS63 peripherals match the SoC memory map. This is a pure address-mapping
    /// invariant (no MMIO access), so it is identical on hardware and in QEMU and
    /// can never be flaky. Guards against a regenerated PAC silently shifting a
    /// peripheral window.
    #[test]
    fn pac_peripheral_base_addresses() {
        assert_eq!(pac::Gpio0::PTR as usize, 0x4402_8000, "GPIO0 base moved");
        assert_eq!(pac::Uart0::PTR as usize, 0x4401_0000, "UART0 base moved");
        assert_eq!(pac::Tcxo::PTR as usize, 0x4400_04c0, "TCXO base moved");
    }

    /// Read a real SoC register through the PAC singleton handed over by
    /// `#[init]` and assert a structural fact about it. We read the TCXO status
    /// register and assert the read completed (the value is whatever the bus
    /// returns); the point is that an MMIO load to the TCXO window succeeds
    /// on-target without trapping. Reads only — no writes, no state change.
    #[test]
    fn read_tcxo_status_register(p: pac::Peripherals) {
        // `bits()` performs a volatile 32-bit load from 0x4400_04c0 + offset.
        let status = p.tcxo.tcxo_status().read().bits();
        // The reserved upper bits are not all-ones on a sane bus read; this is a
        // weak-but-real liveness assertion that the load returned bus data rather
        // than the all-ones "no device" pattern.
        assert_ne!(status, 0xFFFF_FFFF, "TCXO status read returned the bus-floating all-ones pattern");
    }
}
