//! On-target (semihosting) **cross-cutting / CPU / PAC smoke** tests for the
//! WS63 RISC-V target.
//!
//! This crate is now the cross-cutting smoke suite: pure-CPU (M/F/CSR) invariants
//! and PAC structural address-map invariants that don't belong to any single HAL
//! driver. The HAL-**driver** on-target tests (GPIO/TCXO/UART/clock/system/timer/
//! DMA) live with the code they exercise, in `hisi-hal/tests/hil.rs`, where
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
    #[cfg(feature = "chip-bs21")]
    use bs2x_pac as pac;
    #[cfg(feature = "chip-ws63")]
    use ws63_pac as pac;

    /// `#[init]` runs before every test. It takes the HAL singleton once and
    /// hands it to tests that exercise owned peripheral capabilities. The HAL
    /// obtains this state through the PAC's critical-section-guarded `take()`,
    /// so the underlying singleton remains unique.
    #[init]
    fn init() -> hisi_hal::peripherals::Peripherals {
        hisi_hal::peripherals::Peripherals::take()
            .expect("HAL Peripherals::take() returned None on first call")
    }

    /// Proves the PAC-driven WS63 SPACC implementation against public SM3 and
    /// HMAC-SM3 known-answer vectors. This is silicon evidence for the typed
    /// algorithm capability; host tests only prove the software oracle and
    /// register configuration model.
    #[cfg(feature = "chip-ws63")]
    #[test]
    fn spacc_sm3_hmac_sm3_known_answer_vectors(p: hisi_hal::peripherals::Peripherals) {
        use hisi_crypto::{TryHashAlgorithm, TryMacAlgorithm, algorithm};
        use hisi_crypto_ws63::{Ws63Crypto, Ws63CryptoResources, Ws63CryptoStorage};
        use static_cell::StaticCell;

        static STORAGE: StaticCell<Ws63CryptoStorage> = StaticCell::new();

        let storage = STORAGE.init(Ws63CryptoStorage::new());
        let crypto = Ws63Crypto::new(Ws63CryptoResources::new(p.KM, p.SPACC, p.TRNG, storage));

        let mut digest = [0u8; 32];
        TryHashAlgorithm::<algorithm::Sm3, 32>::hash(&crypto, &[b"abc"], &mut digest)
            .expect("WS63 SPACC SM3 operation failed");
        assert_eq!(
            digest,
            [
                0x66, 0xc7, 0xf0, 0xf4, 0x62, 0xee, 0xed, 0xd9, 0xd1, 0xf2, 0xd4, 0x6b, 0xdc, 0x10,
                0xe4, 0xe2, 0x41, 0x67, 0xc4, 0x87, 0x5c, 0xf2, 0xf7, 0xa2, 0x29, 0x7d, 0xa0, 0x2b,
                0x8f, 0x4b, 0xa8, 0xe0,
            ],
            "WS63 SPACC SM3 digest mismatch"
        );

        let mut mac = [0u8; 32];
        TryMacAlgorithm::<algorithm::Sm3, 32>::mac(&crypto, &[0x0b; 20], &[b"abc"], &mut mac)
            .expect("WS63 SPACC HMAC-SM3 operation failed");
        assert_eq!(
            mac,
            [
                0x8e, 0xc4, 0xd9, 0xf9, 0xe5, 0x15, 0x9d, 0x52, 0xd8, 0xb7, 0xf8, 0xe8, 0xe6, 0x81,
                0xa6, 0x2e, 0xcd, 0x2f, 0xb0, 0xcb, 0x58, 0xba, 0x55, 0x4e, 0xe5, 0x6c, 0x96, 0x2d,
                0x0f, 0xa5, 0xda, 0xa1,
            ],
            "WS63 SPACC HMAC-SM3 mismatch"
        );
    }

    /// Proves that the typed AES-CMAC capability composes the WS63 SPACC block
    /// cipher correctly across fragmented input, including RFC 4493 subkeys.
    #[cfg(feature = "chip-ws63")]
    #[test]
    fn spacc_aes_cmac_known_answer_vector(p: hisi_hal::peripherals::Peripherals) {
        use hisi_crypto::{TryMacAlgorithm, algorithm};
        use hisi_crypto_ws63::{Ws63Crypto, Ws63CryptoResources, Ws63CryptoStorage};
        use static_cell::StaticCell;

        static STORAGE: StaticCell<Ws63CryptoStorage> = StaticCell::new();

        let storage = STORAGE.init(Ws63CryptoStorage::new());
        let crypto = Ws63Crypto::new(Ws63CryptoResources::new(p.KM, p.SPACC, p.TRNG, storage));
        let key = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf,
            0x4f, 0x3c,
        ];
        let message = [
            0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96, 0xe9, 0x3d, 0x7e, 0x11, 0x73, 0x93,
            0x17, 0x2a, 0xae, 0x2d, 0x8a, 0x57, 0x1e, 0x03, 0xac, 0x9c, 0x9e, 0xb7, 0x6f, 0xac,
            0x45, 0xaf, 0x8e, 0x51, 0x30, 0xc8, 0x1c, 0x46, 0xa3, 0x5c, 0xe4, 0x11, 0xe5, 0xfb,
            0xc1, 0x19, 0x1a, 0x0a, 0x52, 0xef, 0xf6, 0x9f, 0x24, 0x45, 0xdf, 0x4f, 0x9b, 0x17,
            0xad, 0x2b, 0x41, 0x7b, 0xe6, 0x6c, 0x37, 0x10,
        ];
        let vectors = [
            (
                0,
                [
                    0xbb, 0x1d, 0x69, 0x29, 0xe9, 0x59, 0x37, 0x28, 0x7f, 0xa3, 0x7d, 0x12, 0x9b,
                    0x75, 0x67, 0x46,
                ],
            ),
            (
                16,
                [
                    0x07, 0x0a, 0x16, 0xb4, 0x6b, 0x4d, 0x41, 0x44, 0xf7, 0x9b, 0xdd, 0x9d, 0xd0,
                    0x4a, 0x28, 0x7c,
                ],
            ),
            (
                40,
                [
                    0xdf, 0xa6, 0x67, 0x47, 0xde, 0x9a, 0xe6, 0x30, 0x30, 0xca, 0x32, 0x61, 0x14,
                    0x97, 0xc8, 0x27,
                ],
            ),
            (
                64,
                [
                    0x51, 0xf0, 0xbe, 0xbf, 0x7e, 0x3b, 0x9d, 0x92, 0xfc, 0x49, 0x74, 0x17, 0x79,
                    0x36, 0x3c, 0xfe,
                ],
            ),
        ];
        for (length, expected) in vectors {
            let first = core::cmp::min(length, 5);
            let second = core::cmp::min(length, 23);
            let mut mac = [0u8; 16];
            TryMacAlgorithm::<algorithm::AesCmac, 16>::mac(
                &crypto,
                &key,
                &[
                    &message[..first],
                    &message[first..second],
                    &message[second..length],
                ],
                &mut mac,
            )
            .expect("WS63 SPACC AES-CMAC operation failed");
            assert_eq!(mac, expected, "WS63 SPACC AES-CMAC mismatch");
        }
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
        assert_eq!(
            pac::CldoCrg::PTR as usize,
            0x4400_1100,
            "CLDO_CRG base moved"
        );
        assert_eq!(
            pac::SysCtl0::PTR as usize,
            0x4000_0000,
            "SYS_CTL0 base moved"
        );
    }

    /// More PAC base-address structural facts, extending
    /// `pac_peripheral_base_addresses`: a few additional peripheral windows whose
    /// HAL drivers are exercised by `hisi-hal/tests/hil.rs`. Pure
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
        assert_eq!(
            pac::GlbCtlM::PTR as usize,
            0x4000_2000,
            "GLB_CTL_M base moved"
        );
    }
}
