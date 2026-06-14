//! On-target (semihosting) **cross-cutting / CPU / PAC smoke** tests for the
//! WS63 RISC-V target.
//!
//! This crate is now the cross-cutting smoke suite: pure-CPU (M/F/CSR) invariants
//! and PAC structural address-map invariants that don't belong to any single HAL
//! driver. The HAL-**driver** on-target tests (GPIO/TCXO/UART/clock/system/timer/
//! DMA) live with the code they exercise, in `hisi-riscv-hal/tests/hil.rs`, where
//! they ship + run with the HAL and inherit its chip gating.
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
    // Chip-selected PAC alias: the suite names `pac::{Peripherals, Gpio0, ...}`
    // chip-agnostically and the active chip feature picks the concrete PAC.
    #[cfg(feature = "chip-ws63")]
    use ws63_pac as pac;
    #[cfg(feature = "chip-bs21")]
    use bs2x_pac as pac;

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
    ///
    /// WS63-specific PAC addresses → gated `chip-ws63`. A `#[cfg(feature =
    /// "chip-bs21")]` sibling with the BS21 base addresses can be added when a
    /// BS21 board exists.
    #[cfg(feature = "chip-ws63")]
    #[test]
    fn pac_peripheral_base_addresses() {
        assert_eq!(pac::Gpio0::PTR as usize, 0x4402_8000, "GPIO0 base moved");
        assert_eq!(pac::Gpio1::PTR as usize, 0x4402_9000, "GPIO1 base moved");
        assert_eq!(pac::Gpio2::PTR as usize, 0x4402_a000, "GPIO2 base moved");
        assert_eq!(pac::Uart0::PTR as usize, 0x4401_0000, "UART0 base moved");
        assert_eq!(pac::Tcxo::PTR as usize, 0x4400_04c0, "TCXO base moved");
        assert_eq!(pac::Timer::PTR as usize, 0x4400_2000, "TIMER base moved");
        assert_eq!(pac::Dma::PTR as usize, 0x4a00_0000, "DMA (MDMA) base moved");
        assert_eq!(pac::Sdma::PTR as usize, 0x520a_0000, "SDMA base moved");
        assert_eq!(pac::CldoCrg::PTR as usize, 0x4400_1100, "CLDO_CRG base moved");
        assert_eq!(pac::SysCtl0::PTR as usize, 0x4000_0000, "SYS_CTL0 base moved");
    }

    /// More PAC base-address structural facts, extending
    /// `pac_peripheral_base_addresses`: a few additional peripheral windows whose
    /// HAL drivers are exercised by `hisi-riscv-hal/tests/hil.rs`. Pure
    /// compile-time constants — identical on hardware and QEMU, never flaky.
    /// (Expected values verified against ws63-pac's `pub type X = Periph<.., 0x..>`
    /// definitions.)
    ///
    /// WS63-specific PAC addresses → gated `chip-ws63`. A `#[cfg(feature =
    /// "chip-bs21")]` sibling with the BS21 base addresses can be added when a
    /// BS21 board exists.
    #[cfg(feature = "chip-ws63")]
    #[test]
    fn pac_peripheral_base_addresses_extra() {
        assert_eq!(pac::I2c0::PTR as usize, 0x4401_8000, "I2C0 base moved");
        assert_eq!(pac::Spi0::PTR as usize, 0x4402_0000, "SPI0 base moved");
        assert_eq!(pac::Pwm::PTR as usize, 0x4402_4000, "PWM base moved");
        assert_eq!(pac::Wdt::PTR as usize, 0x4000_6000, "WDT base moved");
        assert_eq!(pac::Rtc::PTR as usize, 0x5702_4000, "RTC base moved");
        assert_eq!(pac::GlbCtlM::PTR as usize, 0x4000_2000, "GLB_CTL_M base moved");
    }
}
