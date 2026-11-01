//! QEMU-only temperature profile. No physical analog/clock/thermal-service handoff.
#![no_std]
#![no_main]
use core::{cell::RefCell, num::NonZeroU16};
use embedded_hal::delay::DelayNs;
use hisi_hal::Peripherals;
use hisi_hal::peripherals::Uart2;
use hisi_hal::timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks};
use hisi_hal::tsensor::{Error, TempSensor};
use hisi_hal::uart::Uart;
use hisi_riscv_rt::entry;
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
        let mut budget = 1_000_000;
        while uart.try_write_byte(byte).is_err() {
            budget -= 1;
            assert!(budget != 0, "UART stopped");
        }
    }
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    // QEMU-only synthetic clock, power and thermal-service ownership.
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
    let mut delay = TimerDelay(&timer);
    emit(&uart, b"WS53 TEMPERATURE QEMU begin\n");
    match TempSensor::new(unsafe { p.TSENSOR.assume_application_ownership() }) {
        Err((error, mut sensor)) => {
            assert_eq!(error, Error::UnsupportedControl(0xdead));
            assert_eq!(sensor.try_read(), Err(Error::NotConfigured));
            assert_eq!(sensor.recover(), Err(Error::UnsupportedControl(0xdead)));
            let token = sensor.release_without_stopping();
            assert!(matches!(
                TempSensor::new(token),
                Err((Error::UnsupportedControl(0xdead), _))
            ));
            emit(
                &uart,
                b"WS53 PASS TEMPERATURE unknown_control=1 retained=1 writes=0\n",
            );
        }
        Ok(mut sensor) => match sensor.read_bounded(NonZeroU16::new(1000).unwrap(), &mut delay) {
            Err(error) => {
                assert_eq!(error, Error::Timeout);
                assert_eq!(
                    sensor.read_bounded(NonZeroU16::new(3).unwrap(), &mut delay),
                    Err(Error::Timeout)
                );
                sensor.recover().unwrap();
                let token = sensor.release_without_stopping();
                let mut attached =
                    TempSensor::new(token).unwrap_or_else(|_| panic!("reattach failed"));
                assert_eq!(attached.try_read(), Ok(None));
                emit(
                    &uart,
                    b"WS53 PASS TEMPERATURE clock_timeout=1 retry=1 attach_no_restart=1\n",
                );
            }
            Ok(sample) => {
                for _ in 0..3 {
                    assert_eq!(sensor.try_read(), Ok(Some(sample)));
                }
                let token = sensor.release_without_stopping();
                let mut attached =
                    TempSensor::new(token).unwrap_or_else(|_| panic!("reattach failed"));
                assert_eq!(attached.try_read(), Ok(Some(sample)));
                match sample.raw() {
                    933 => {
                        let nominal = sample.nominal_temperature().unwrap();
                        assert_eq!(
                            (nominal.centi_celsius(), nominal.whole_celsius()),
                            (12500, 125)
                        );
                        emit(&uart, b"WS53 PASS TEMPERATURE raw=933 nominal_c=125 sticky=1 attach_no_restart=1\n");
                    }
                    1023 => {
                        assert!(sample.nominal_temperature().is_none());
                        emit(&uart, b"WS53 PASS TEMPERATURE raw=1023 outside_reference=1 attach_no_restart=1\n");
                    }
                    _ => panic!("unexpected synthetic input"),
                }
            }
        },
    }
    timer.borrow_mut().disable(TimerChannel::Channel1);
    emit(&uart, b"WS53 TEMPERATURE QEMU done\n");
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
