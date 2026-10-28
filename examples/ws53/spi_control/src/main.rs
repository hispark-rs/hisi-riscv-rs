//! QEMU-only SPI fixture: synthetic pad permissions, no board route or native CS.
#![no_std]
#![no_main]
use core::{cell::RefCell, num::NonZeroU16};
use embedded_hal::{delay::DelayNs, spi::SpiBus};
use hisi_hal::peripherals::Uart2;
use hisi_hal::pinctrl::PadControl;
use hisi_hal::soc::ws53::pins::Pad;
use hisi_hal::spi::{
    Config, Error, Failure, MODE_0, MODE_1, MODE_2, MODE_3, Pins, Spi, SpiHz, Wait, Word,
};
use hisi_hal::timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks};
use hisi_hal::uart::Uart;
use hisi_hal::{Peripherals, interrupt};
use hisi_riscv_rt::{entry, interrupt::ExternalInterrupt};

#[derive(Clone, Copy)]
struct TimerDelay<'a, 'd>(&'a RefCell<TimerDriver<'d>>);
impl DelayNs for TimerDelay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        let start = self
            .0
            .borrow_mut()
            .current_value(TimerChannel::Channel1)
            .unwrap();
        for _ in 0..10_000_000 {
            let now = self
                .0
                .borrow_mut()
                .current_value(TimerChannel::Channel1)
                .unwrap();
            if start.wrapping_sub(now) >= ticks {
                return;
            }
        }
        panic!("TIMER stopped");
    }
}
fn emit(uart: &Uart<'_, Uart2<'_>>, bytes: &[u8]) {
    for &byte in bytes {
        let mut remaining = 1_000_000;
        while uart.try_write_byte(byte).is_err() {
            remaining -= 1;
            assert!(remaining != 0, "UART stopped");
        }
    }
}
fn exercise<T, W: Word, D: DelayNs>(bus: &mut Spi<'_, T, W, D>) {
    let mask = if W::DFS == 7 { 0xff } else { u32::MAX };
    let input = [W::from_fifo(0x12345678); 100];
    let mut output = [W::default(); 100];
    bus.transfer(&mut output, &input).unwrap();
    for value in output {
        assert_eq!(value.into_fifo(), 0xb791f3dd & mask);
    }
    let mut words = [W::from_fifo(0x3c); 5];
    bus.transfer_in_place(&mut words).unwrap();
    for value in words {
        assert_eq!(value.into_fifo(), 0xa5a5a599 & mask);
    }
    // Longer read sends zeros, shorter read still drains all replies.
    let mut read = [W::default(); 3];
    bus.transfer(&mut read, &[W::from_fifo(0x56)]).unwrap();
    assert_eq!(read[0].into_fifo(), 0xa5a5a5f3 & mask);
    assert_eq!(read[2].into_fifo(), 0xa5a5a5a5 & mask);
    bus.transfer(&mut read[..1], &input[..5]).unwrap();
    bus.write(&input[..5]).unwrap();
    bus.read(&mut read).unwrap();
    for value in read {
        assert_eq!(value.into_fifo(), 0xa5a5a5a5 & mask);
    }
    bus.transfer(&mut [], &[]).unwrap();
    bus.flush().unwrap();
    bus.recover().unwrap();
}
macro_rules! exercise_instance {
    ($token:expr, $pins:expr, $ctor:ident, $delay:expr) => {{
        let mut token = $token;
        let mut pins = $pins;
        for mode in [MODE_0, MODE_1, MODE_2, MODE_3] {
            let config = Config::new(
                SpiHz::try_from_hz(10_000_000).unwrap(),
                mode,
                NonZeroU16::new(32).unwrap(),
            );
            let mut bus = Spi::<_, u8, _>::$ctor(token, pins, config, $delay)
                .unwrap_or_else(|_| panic!("SPI8 init failed"));
            exercise(&mut bus);
            (token, pins) = bus
                .try_free()
                .unwrap_or_else(|_| panic!("SPI8 close failed"));
            let mut bus = Spi::<_, u32, _>::$ctor(token, pins, config, $delay)
                .unwrap_or_else(|_| panic!("SPI32 init failed"));
            exercise(&mut bus);
            (token, pins) = bus
                .try_free()
                .unwrap_or_else(|_| panic!("SPI32 close failed"));
        }
        pins.into_pads()
    }};
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    // Only the QEMU profile establishes these clocks/routes/XIP permissions.
    // These pads are distinct synthetic permissions, NOT a physical pin map.
    let uart = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    let timer = RefCell::new(TimerDriver::new(unsafe {
        p.TIMER.assume_application_ownership()
    }));
    timer.borrow_mut().configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.borrow_mut().enable(TimerChannel::Channel1);
    let delay = TimerDelay(&timer);
    let pins0 = unsafe {
        Pins::from_configured(
            PadControl::claim(Pad::Mgpio3).unwrap(),
            PadControl::claim(Pad::Mgpio4).unwrap(),
            PadControl::claim(Pad::Mgpio5).unwrap(),
        )
    };
    let pins1 = unsafe {
        Pins::from_configured(
            PadControl::claim(Pad::Mgpio8).unwrap(),
            PadControl::claim(Pad::Mgpio9).unwrap(),
            PadControl::claim(Pad::Mgpio10).unwrap(),
        )
    };
    emit(&uart, b"WS53 SPI QEMU begin\n");
    let spi0 = unsafe { p.SPI0.assume_application_ownership() };
    if cfg!(feature = "clock-stall-smoke") {
        let config = Config::new(
            SpiHz::try_from_hz(10_000_000).unwrap(),
            MODE_0,
            NonZeroU16::new(3).unwrap(),
        );
        let mut bus = Spi::<_, u8, _>::new_spi0(spi0, pins0, config, delay)
            .unwrap_or_else(|_| panic!("disabled SPI init failed"));
        assert_eq!(
            bus.write(&[3]),
            Err(Error::RecoveryFailed(Failure::Timeout(Wait::Receive)))
        );
        assert!(bus.is_poisoned());
        assert_eq!(bus.write(&[4]), Err(Error::Poisoned));
        assert_eq!(
            bus.recover(),
            Err(Error::Failure(Failure::Timeout(Wait::Disable)))
        );
        let (error, still_owned) = match bus.try_free() {
            Err(retained) => retained,
            Ok(_) => panic!("stopped SPI incorrectly released"),
        };
        assert_eq!(error, Error::Failure(Failure::Timeout(Wait::Disable)));
        assert!(still_owned.is_poisoned());
        drop(still_owned); // bounded best effort, no inactive-bus claim
    } else {
        let (sclk, mosi, miso) = exercise_instance!(spi0, pins0, new_spi0, delay);
        assert_eq!(
            (sclk.pad(), mosi.pad(), miso.pad()),
            (Pad::Mgpio3, Pad::Mgpio4, Pad::Mgpio5)
        );
    }
    let (sclk, mosi, miso) = exercise_instance!(
        unsafe { p.QSPI1.assume_application_ownership() },
        pins1,
        new_qspi1,
        delay
    );
    assert_eq!(
        (sclk.pad(), mosi.pad(), miso.pad()),
        (Pad::Mgpio8, Pad::Mgpio9, Pad::Mgpio10)
    );
    assert!(!interrupt::is_enabled(ExternalInterrupt::SPI_3));
    assert!(!interrupt::is_enabled(ExternalInterrupt::QSPI0_2CS));
    if cfg!(feature = "clock-stall-smoke") {
        emit(
            &uart,
            b"WS53 PASS SPI clock_stall rx_timeout=1 poisoned=1 retained=1 other_instance=1\n",
        );
    } else {
        emit(
            &uart,
            b"WS53 PASS SPI instances=2 words8_32=1 modes=4 long_transfer=100 reclaim=1\n",
        );
    }
    timer.borrow_mut().disable(TimerChannel::Channel1);
    emit(&uart, b"WS53 SPI QEMU done\n");
    loop {
        riscv::asm::wfi();
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
