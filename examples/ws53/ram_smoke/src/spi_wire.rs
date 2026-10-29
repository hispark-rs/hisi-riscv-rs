//! SPI0 real pad experiment, only analyzer inputs attached. No external slave.
//! SDK 163d9150: MGPIO8/9 mode2 SCLK/TXD, MGPIO12 mode1 RXD.
//! MGPIO17/D5 marker; AGPIO4/D1 caller-managed software CS. No SFC/QSPI writes.
use super::*;
use core::{cell::RefCell, num::NonZeroU16};
use embedded_hal::{delay::DelayNs, spi::SpiBus};
use hisi_hal::{
    gpio::{GpioDriver, Output},
    peripherals::{Gpio0, Gpio1, Spi0, Unclaimed},
    pinctrl::{Config as PadConfig, PadControl, Pull},
    soc::ws53::pins::{MuxMode, Pad},
    spi::{Config, MODE_0, MODE_1, MODE_2, MODE_3, Pins, Spi, SpiHz, Word},
};

#[unsafe(no_mangle)]
pub static mut WS53_SPI_WIRE: [u32; 352] = [0; 352];
pub const TX8: [u8; 8] = [0x96, 0x3c, 0xa5, 0x00, 0xff, 0x81, 0x7e, 0x42];
pub const TX32: [u32; 4] = [0x963ca500, 0xff817e42, 0x13579bdf, 0x02468ace];
fn put(index: usize, value: u32) {
    unsafe {
        core::ptr::addr_of_mut!(WS53_SPI_WIRE)
            .cast::<u32>()
            .add(index)
            .write_volatile(value)
    }
}
fn now(timer: &RefCell<&mut TimerDriver<'_>>) -> u32 {
    timer
        .borrow_mut()
        .current_value(TimerChannel::Channel1)
        .unwrap()
}
#[derive(Clone, Copy)]
struct Delay<'a, 'b, 'd>(&'a RefCell<&'b mut TimerDriver<'d>>);
impl DelayNs for Delay<'_, '_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        let start = now(self.0);
        for _ in 0..1_000_000 {
            if start.wrapping_sub(now(self.0)) >= ticks {
                return;
            }
        }
        panic!("SPI independent TIMER stopped");
    }
}
fn prepare(pad: Pad, mode: u8, pull: Pull) -> (PadControl<'static>, PadConfig) {
    let mut owner = unsafe { PadControl::claim(pad) }.unwrap();
    let saved = owner.configuration().unwrap();
    owner
        .configure(
            PadConfig::new(
                pad.select_mode(MuxMode::from_code(mode).unwrap()).unwrap(),
                pad.drive_setting(0).unwrap(),
                pull,
                true,
                saved.schmitt_enabled(),
            )
            .unwrap(),
        )
        .unwrap();
    (owner, saved)
}
fn restore(mut owner: PadControl<'_>, saved: PadConfig) {
    owner.configure(saved).unwrap();
    assert_eq!(owner.configuration().unwrap(), saved);
}
fn phase<W: Word + PartialEq, const N: usize>(
    driver: &mut Spi<'_, Spi0<'_>, W, Delay<'_, '_, '_>>,
    timer: &RefCell<&mut TimerDriver<'_>>,
    marker: &mut Output<'_, '_, '_>,
    cs: &mut Output<'_, '_, '_>,
    index: usize,
    tx: &[W; N],
    high: bool,
) {
    let base = 32 + index * 20;
    let mut rx = [W::default(); N];
    let before = now(timer);
    marker.set_high();
    riscv::asm::fence();
    let post_high = now(timer);
    cs.set_low();
    riscv::asm::fence();
    let result = driver.transfer(&mut rx, tx);
    riscv::asm::fence();
    cs.set_high();
    let pre_low = now(timer);
    marker.set_low();
    riscv::asm::fence();
    let after = now(timer);
    put(base, index as u32);
    put(base + 1, if W::DFS == 7 { 8 } else { 32 });
    put(base + 2, (index % 4) as u32);
    put(
        base + 3,
        if (index / 4).is_multiple_of(2) {
            100_000
        } else {
            500_000
        },
    );
    put(base + 4, u32::from(high));
    put(base + 5, u32::from(result.is_err()));
    put(base + 6, before);
    put(base + 7, post_high);
    put(base + 8, pre_low);
    put(base + 9, after);
    put(base + 10, N as u32);
    for (i, value) in rx.into_iter().enumerate() {
        put(base + 11 + i, value.into_fifo());
    }
    let r = unsafe { &*ws53_pac::Spi0::PTR };
    put(base + 19, r.status().read().bits());
    result.unwrap_or_else(|_| panic!("SPI physical transfer failed"));
    let expected = if high {
        if W::DFS == 7 { 0xff } else { u32::MAX }
    } else {
        0
    };
    assert!(
        rx.into_iter().all(|value| value.into_fifo() == expected),
        "SPI physical MISO level differs"
    );
    assert!(!driver.is_poisoned());
    Delay(timer).delay_ms(2);
    put(1, (index + 1) as u32);
}
pub fn check(
    spi: Unclaimed<Spi0<'static>>,
    gpio0: Unclaimed<Gpio0<'static>>,
    gpio1: Unclaimed<Gpio1<'static>>,
    timer: &mut TimerDriver<'_>,
    uart: &Uart<'_, Uart2<'_>>,
) {
    put(0, 0x53c40001);
    assert!(!riscv::register::mstatus::read().mie());
    // The bank must acquire its clock before the first raw PAC observation.
    // FlashBoot does not provide the full vendor application's GPIO1 setup.
    let mut bank1 = GpioDriver::new_gpio1(unsafe { gpio1.assume_application_ownership() })
        .unwrap_or_else(|_| panic!("SPI GPIO1 clock handoff failed"));
    let irq = hisi_hal::interrupt::Interrupt::SPI_3;
    assert!(!interrupt::is_enabled(irq) && !interrupt::is_pending(irq));
    let crg = unsafe { &*ws53_pac::MCrg::PTR };
    let old_gate = crg.m_clken1().read().bits();
    let old_clock = crg.spi_clock().read().bits();
    let old_mode = crg.spi_device_mode().read().bits();
    put(4, u32::from(old_gate));
    put(5, u32::from(old_clock));
    put(6, u32::from(old_mode));
    let g0 = unsafe { &*ws53_pac::Gpio0::PTR };
    let g1 = unsafe { &*ws53_pac::Gpio1::PTR };
    let directions = [
        g0.gpio_sw_oen().read().bits(),
        g1.gpio_sw_oen().read().bits(),
    ];
    put(10, directions[0]);
    put(11, directions[1]);
    assert_ne!(
        directions[0] & (1 << 17),
        0,
        "SPI marker must start released"
    );
    assert_ne!(directions[1] & (1 << 4), 0, "SPI CS must start released");
    assert_eq!(g0.gpio_int_en().read().bits(), 0);
    assert_eq!(g1.gpio_int_en().read().bits(), 0);
    assert_eq!(g0.gpio_sw_ctl().read().bits(), 0);
    assert_eq!(g1.gpio_sw_ctl().read().bits(), 0);
    // Release all SPI0 aliases before selecting one route. Preserve other GPIOs.
    let mask = [2, 3, 4, 8, 9, 10, 12, 13, 17]
        .into_iter()
        .fold(0u32, |m, n| m | (1 << n));
    g0.gpio_sw_oen()
        .modify(|r, w| unsafe { w.pins().bits(r.pins().bits() | mask) });
    g1.gpio_sw_oen()
        .modify(|r, w| unsafe { w.pins().bits(r.pins().bits() | (1 << 4)) });
    let aliases = [
        Pad::Mgpio2,
        Pad::Mgpio3,
        Pad::Mgpio4,
        Pad::Mgpio10,
        Pad::Mgpio13,
    ]
    .map(|pad| prepare(pad, 0, Pull::Down));
    let (sclk, sclk_saved) = prepare(Pad::Mgpio8, 2, Pull::Down);
    let (mosi, mosi_saved) = prepare(Pad::Mgpio9, 2, Pull::Down);
    let (miso, miso_saved) = prepare(Pad::Mgpio12, 1, Pull::Down);
    let (marker, marker_saved) = prepare(Pad::Mgpio17, 0, Pull::Down);
    let (cs, cs_saved) = prepare(Pad::Agpio4, 0, Pull::Up);
    let mut bank0 = GpioDriver::new_gpio0(unsafe { gpio0.assume_application_ownership() });
    let mut marker = bank0
        .init_output(
            marker
                .into_gpio()
                .unwrap_or_else(|_| panic!("SPI marker mux")),
            false,
        )
        .unwrap_or_else(|_| panic!("SPI marker GPIO"));
    let mut cs = bank1
        .init_output(
            cs.into_gpio().unwrap_or_else(|_| panic!("SPI CS mux")),
            true,
        )
        .unwrap_or_else(|_| panic!("SPI CS GPIO"));
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    let cell = RefCell::new(timer);
    let mut token = unsafe { spi.assume_application_ownership() };
    let mut pins = unsafe { Pins::from_configured(sclk, mosi, miso) };
    for index in 0..16 {
        let (sclk, mosi, mut miso) = pins.into_pads();
        let high = (index + index / 8) % 2 == 1;
        let config = miso.configuration().unwrap();
        miso.configure(
            PadConfig::new(
                config.mode(),
                Pad::Mgpio12.drive_setting(0).unwrap(),
                if high { Pull::Up } else { Pull::Down },
                true,
                config.schmitt_enabled(),
            )
            .unwrap(),
        )
        .unwrap();
        pins = unsafe { Pins::from_configured(sclk, mosi, miso) };
        Delay(&cell).delay_ms(2);
        let config = Config::new(
            SpiHz::try_from_hz(if (index / 4) % 2 == 1 {
                500_000
            } else {
                100_000
            })
            .unwrap(),
            [MODE_0, MODE_1, MODE_2, MODE_3][index % 4],
            NonZeroU16::new(5000).unwrap(),
        );
        macro_rules! exercise {
            ($word:ty, $data:ident) => {{
                let mut driver = Spi::<_, $word, _>::new_spi0(token, pins, config, Delay(&cell))
                    .unwrap_or_else(|_| panic!("SPI physical constructor failed"));
                phase(
                    &mut driver,
                    &cell,
                    &mut marker,
                    &mut cs,
                    index,
                    &$data,
                    high,
                );
                (token, pins) = driver
                    .try_free()
                    .unwrap_or_else(|_| panic!("SPI physical shutdown failed"));
            }};
        }
        if index < 8 {
            exercise!(u8, TX8);
        } else {
            exercise!(u32, TX32);
        }
    }
    let r = unsafe { &*ws53_pac::Spi0::PTR };
    put(16, r.enable().read().bits());
    put(17, r.interrupt_mask().read().bits());
    put(18, r.dma_control().read().bits());
    put(19, r.status().read().bits());
    assert!(!r.enable().read().start().bit());
    assert_eq!(r.interrupt_mask().read().bits(), 0);
    assert_eq!(r.dma_control().read().bits(), 0);
    let (sclk, mosi, miso) = pins.into_pads();
    restore(sclk, sclk_saved);
    restore(mosi, mosi_saved);
    restore(miso, miso_saved);
    let _ = cs.free();
    let _ = marker.free();
    restore(
        unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap(),
        marker_saved,
    );
    restore(unsafe { PadControl::claim(Pad::Agpio4) }.unwrap(), cs_saved);
    for (owner, saved) in aliases {
        restore(owner, saved);
    }
    g0.gpio_sw_oen()
        .modify(|_, w| unsafe { w.pins().bits(directions[0]) });
    g1.gpio_sw_oen()
        .modify(|_, w| unsafe { w.pins().bits(u16::try_from(directions[1]).unwrap()) });
    crg.spi_clock()
        .modify(|_, w| w.enable().bit(old_clock & 1 != 0));
    crg.spi_device_mode()
        .modify(|_, w| w.master().bit(old_mode & 1 != 0));
    crg.m_clken1()
        .modify(|_, w| w.spi3_ms_enable().bit(old_gate & (1 << 11) != 0));
    put(7, u32::from(crg.m_clken1().read().bits()));
    put(8, u32::from(crg.spi_clock().read().bits()));
    put(9, u32::from(crg.spi_device_mode().read().bits()));
    put(12, g0.gpio_sw_oen().read().bits());
    put(13, g1.gpio_sw_oen().read().bits());
    assert_eq!(crg.m_clken1().read().bits(), old_gate);
    assert_eq!(crg.spi_clock().read().bits(), old_clock);
    assert_eq!(crg.spi_device_mode().read().bits(), old_mode);
    assert_eq!(g0.gpio_sw_oen().read().bits(), directions[0]);
    assert_eq!(g1.gpio_sw_oen().read().bits(), directions[1]);
    cell.borrow_mut().disable(TimerChannel::Channel1);
    cell.borrow_mut().clear_interrupt(TimerChannel::Channel1);
    interrupt::clear_pending(TimerChannel::Channel1.interrupt());
    put(14, 1);
    put(
        15,
        u32::from(interrupt::is_enabled(irq) || interrupt::is_pending(irq)),
    );
    put(2, 1);
    emit(uart, b"WS53 SPI WIRE OBSERVED phases=16 restored=1\r\n");
    let _ = token;
}
