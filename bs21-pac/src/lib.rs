//! Peripheral Access Crate for HiSilicon **BS21 / BS2X** (RISC-V, SparkLink/NearLink).
//!
//! BS21's UART/timer/GPIO/etc. are the **same versioned IP blocks** as WS63 (UART
//! v151, timer v150, GPIO v150 — verified against the fbb_bs2x SDK), so this crate
//! **reuses [`ws63_pac`]'s register-block field definitions** and only redefines:
//! - the peripheral **base addresses** (BS21 lives in the 0x52xx_xxxx / 0x57xx_xxxx
//!   space, vs WS63's 0x44xx_xxxx),
//! - the **interrupt map** (`chip_core_irq.h`, `LOCAL_INTERRUPT0 = 26`), and
//! - the [`Peripherals`] set.
//!
//! The interrupt *architecture* (HiSilicon "HimiDeer" LOCI core: mie 26-31 + custom
//! LOCI ≥32) is identical to WS63, so `ws63-hal`'s `interrupt` module works unchanged.
#![no_std]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

#[doc(hidden)]
pub use ws63_pac::generic;
use ws63_pac::generic::Periph;

// Reuse the identical versioned IP register-block modules from ws63-pac.
#[doc(inline)]
pub use ws63_pac::{
    cldo_crg, dma, glb_ctl_m, gpio0, i2c0, pwm, rtc, spi0, sys_ctl0, tcxo, timer, trng, uart0, wdt,
};

#[doc = r"Number available for configuring interrupt priority"]
pub const NVIC_PRIO_BITS: u8 = 3;

pub mod interrupt {
    //! BS21 external interrupts (`chip_core_irq.h`; `LOCAL_INTERRUPT0 = 26`). The
    //! HiSilicon HimiDeer split is identical to WS63: IRQ 26-31 are standard `mie`
    //! bits, IRQ ≥32 are custom LOCI-delivered.
    #[cfg(target_arch = "riscv32")]
    pub use riscv::interrupt::Exception;
    #[cfg(target_arch = "riscv32")]
    pub use riscv::interrupt::Interrupt as CoreInterrupt;
    #[cfg(target_arch = "riscv32")]
    pub use riscv::{
        ExceptionNumber, HartIdNumber, InterruptNumber, PriorityNumber,
        interrupt::{disable, enable, free, nested},
    };
    #[cfg(target_arch = "riscv32")]
    pub type Trap = riscv::interrupt::Trap<CoreInterrupt, Exception>;

    #[doc = r" External interrupt sources (BS21)."]
    #[cfg_attr(target_arch = "riscv32", riscv :: pac_enum (unsafe ExternalInterruptNumber))]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ExternalInterrupt {
        BT_INT0 = 26,
        BT_INT1 = 27,
        GADC_DONE = 28,
        GADC_ALARM = 29,
        MCU_PCLR_LOCK = 32,
        ULP_GPIO = 33,
        GPIO_0 = 34,
        GPIO_1 = 35,
        BT_TOGGLE_POS = 36,
        BT_TOGGLE_NEG = 37,
        KEY_SCAN_LOW_POWER = 38,
        UART_0 = 39,
        MCU_SIMO1P1_VSET = 40,
        UART_1 = 41,
        UART_2 = 42,
        QSPI0_2CS = 43,
        PDM = 44,
        KEY_SCAN = 46,
        M_WAKEUP = 47,
        M_SLEEP = 48,
        RTC_0 = 49,
        RTC_1 = 50,
        RTC_2 = 51,
        RTC_3 = 52,
        TIMER_0 = 53,
        TIMER_1 = 54,
        TIMER_2 = 55,
        TIMER_3 = 56,
        M_SDMA = 57,
        SPI_M_S_0 = 59,
        SPI_M_S_1 = 60,
        SPI_M = 61,
        I2C_0 = 62,
        I2C_1 = 63,
        BT_BB_BT = 64,
        BT_BB_BLE = 65,
        BT_BB_GLE = 66,
        I2S = 67,
        RF_PRT = 68,
        NFC = 69,
        SEC = 70,
        PWM_0 = 71,
        PWM_1 = 72,
        OSC_EN_WKUP = 73,
        OSC_EN_SLEEP = 74,
        PMU_CMU_ERR = 78,
        ULP_INT = 79,
        PMU2_CLK_32K_CALI = 85,
        ULP_WKUP_INT = 86,
        TSENSOR = 87,
        QDEC = 88,
        USB = 89,
    }
}

// ── Peripheral instances at BS21 base addresses (platform_core.h) ────────────
// Register-block types are reused from ws63-pac (identical IP); only the const
// address differs. Blocks BS21 actually drives in milestone 1 (GPIO v150, UART
// v151) are byte-compatible; the rest are address holders for now.

/// GLB_CTL_M — global control (clock/reset), 0x5700_0000.
pub type GlbCtlM = Periph<glb_ctl_m::RegisterBlock, 0x5700_0000>;
/// GPIO0 bank (32 pins), 0x5701_0000.
pub type Gpio0 = Periph<gpio0::RegisterBlock, 0x5701_0000>;
/// GPIO1 bank, 0x5701_4000.
pub type Gpio1 = Periph<gpio0::RegisterBlock, 0x5701_4000>;
/// GPIO2 bank, 0x5701_8000.
pub type Gpio2 = Periph<gpio0::RegisterBlock, 0x5701_8000>;
/// GPIO3 bank, 0x5701_C000.
pub type Gpio3 = Periph<gpio0::RegisterBlock, 0x5701_C000>;
/// GPIO4 bank, 0x5702_0000.
pub type Gpio4 = Periph<gpio0::RegisterBlock, 0x5702_0000>;
/// ULP GPIO bank, 0x5703_0000.
pub type UlpGpio = Periph<gpio0::RegisterBlock, 0x5703_0000>;
/// UART0 (UART_L0), 0x5208_1000.
pub type Uart0 = Periph<uart0::RegisterBlock, 0x5208_1000>;
/// UART1 (UART_H0), 0x5208_0000.
pub type Uart1 = Periph<uart0::RegisterBlock, 0x5208_0000>;
/// UART2 (UART_L1), 0x5208_2000.
pub type Uart2 = Periph<uart0::RegisterBlock, 0x5208_2000>;
/// TIMER block (4 channels at +0x100..+0x400), 0x5200_2000.
pub type Timer = Periph<timer::RegisterBlock, 0x5200_2000>;
/// Watchdog, 0x5200_3000.
pub type Wdt = Periph<wdt::RegisterBlock, 0x5200_3000>;
/// TCXO free-running counter, 0x5700_0200.
pub type Tcxo = Periph<tcxo::RegisterBlock, 0x5700_0200>;
/// I2C0, 0x5208_3000.
pub type I2c0 = Periph<i2c0::RegisterBlock, 0x5208_3000>;
/// I2C1, 0x5208_4000.
pub type I2c1 = Periph<i2c0::RegisterBlock, 0x5208_4000>;
/// SPI0 (SPI_M0), 0x5208_7000.
pub type Spi0 = Periph<spi0::RegisterBlock, 0x5208_7000>;
/// SPI1 (SPI_MS_1), 0x5208_8000.
pub type Spi1 = Periph<spi0::RegisterBlock, 0x5208_8000>;
/// SPI2 (SPI_MS_2), 0x5208_9000.
pub type Spi2 = Periph<spi0::RegisterBlock, 0x5208_9000>;
/// PWM (12 channels), 0x5209_0000.
pub type Pwm = Periph<pwm::RegisterBlock, 0x5209_0000>;
/// M_DMA, 0x5207_0000.
pub type Dma = Periph<dma::RegisterBlock, 0x5207_0000>;
/// S_DMA, 0x520A_0000.
pub type Sdma = Periph<dma::RegisterBlock, 0x520A_0000>;
/// RTC timer, 0x5702_4000.
pub type Rtc = Periph<rtc::RegisterBlock, 0x5702_4000>;
/// TRNG / security common, 0x5200_9000.
pub type Trng = Periph<trng::RegisterBlock, 0x5200_9000>;

// Private (NOT `#[no_mangle]`) so it never clashes with the reused ws63-pac's
// own `DEVICE_PERIPHERALS` symbol (both crates are in the graph under chip-bs21).
static mut DEVICE_PERIPHERALS: bool = false;

/// All BS21 peripheral singletons, handed out once by [`Peripherals::take`].
#[allow(clippy::manual_non_exhaustive)]
pub struct Peripherals {
    pub glb_ctl_m: GlbCtlM,
    pub gpio0: Gpio0,
    pub gpio1: Gpio1,
    pub gpio2: Gpio2,
    pub gpio3: Gpio3,
    pub gpio4: Gpio4,
    pub ulp_gpio: UlpGpio,
    pub uart0: Uart0,
    pub uart1: Uart1,
    pub uart2: Uart2,
    pub timer: Timer,
    pub wdt: Wdt,
    pub tcxo: Tcxo,
    pub i2c0: I2c0,
    pub i2c1: I2c1,
    pub spi0: Spi0,
    pub spi1: Spi1,
    pub spi2: Spi2,
    pub pwm: Pwm,
    pub dma: Dma,
    pub sdma: Sdma,
    pub rtc: Rtc,
    pub trng: Trng,
}

impl Peripherals {
    /// Returns all the peripherals *once*.
    #[cfg(feature = "critical-section")]
    #[inline]
    pub fn take() -> Option<Self> {
        critical_section::with(|_| {
            if unsafe { DEVICE_PERIPHERALS } {
                return None;
            }
            Some(unsafe { Peripherals::steal() })
        })
    }

    /// Unchecked version of [`Peripherals::take`].
    ///
    /// # Safety
    /// Each returned peripheral must be used at most once.
    #[inline]
    pub unsafe fn steal() -> Self {
        unsafe {
            DEVICE_PERIPHERALS = true;
            Peripherals {
                glb_ctl_m: GlbCtlM::steal(),
                gpio0: Gpio0::steal(),
                gpio1: Gpio1::steal(),
                gpio2: Gpio2::steal(),
                gpio3: Gpio3::steal(),
                gpio4: Gpio4::steal(),
                ulp_gpio: UlpGpio::steal(),
                uart0: Uart0::steal(),
                uart1: Uart1::steal(),
                uart2: Uart2::steal(),
                timer: Timer::steal(),
                wdt: Wdt::steal(),
                tcxo: Tcxo::steal(),
                i2c0: I2c0::steal(),
                i2c1: I2c1::steal(),
                spi0: Spi0::steal(),
                spi1: Spi1::steal(),
                spi2: Spi2::steal(),
                pwm: Pwm::steal(),
                dma: Dma::steal(),
                sdma: Sdma::steal(),
                rtc: Rtc::steal(),
                trng: Trng::steal(),
            }
        }
    }
}
