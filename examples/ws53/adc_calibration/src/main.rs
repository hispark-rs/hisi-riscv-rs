//! QEMU-only calibration fixture. Never starts a scan or diagnostic RAM writer.
#![no_std]
#![no_main]
use core::{cell::RefCell, num::NonZeroU16};
use embedded_hal::delay::DelayNs;
use hisi_hal::adc::{Calibrator, Channel, Error, Sample, Stage};
use hisi_hal::peripherals::Uart2;
use hisi_hal::timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks};
use hisi_hal::uart::Uart;
use hisi_hal::{Peripherals, interrupt};
use hisi_riscv_rt::{entry, interrupt::ExternalInterrupt};

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
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    // This external handoff exists only in the explicit QEMU profile. It is
    // not evidence that the installed physical board has no ADC/trace users.
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
    let mut adc = Calibrator::new(unsafe { p.ADC.assume_application_ownership() });
    assert!(adc.result().is_none());
    emit(&uart, b"WS53 ADC QEMU begin\n");
    let polls = NonZeroU16::new(3).unwrap();
    let first = adc.calibrate(polls, &mut delay);
    let line: &[u8] = match first {
        Ok(report) => {
            assert!(report.automatic_offset < 64);
            let n = report.automatic_offset;
            let expected = if n <= 3 {
                0
            } else if n <= 31 {
                n - 3
            } else if n <= 59 {
                n + 3
            } else {
                63
            };
            assert_eq!(report.manual_offset, expected);
            assert_eq!(adc.result(), Some(report));
            for _ in 1..20 {
                assert_eq!(adc.calibrate(polls, &mut delay), Ok(report));
            }
            let word = 0xfffe0003 | (4095 << 2) | (7 << 14);
            let sample = Sample::from_capture_word(word);
            assert_eq!(sample.raw(), word);
            assert_eq!(sample.code(), 4095);
            assert_eq!(sample.channel(), Channel::Channel7);
            b"WS53 PASS ADC calibrated=20 report=1 copied_word=1\n"
        }
        Err(error) => {
            assert_eq!(adc.result(), None);
            // Retry is explicit and bounded, not an automatic success after cleanup.
            assert_eq!(adc.calibrate(polls, &mut delay), Err(error));
            assert_eq!(adc.result(), None);
            match error {
                Error::Timeout(Stage::OffsetAuto) => {
                    b"WS53 PASS ADC offset_auto_timeout attempts=2 no_report=1\n"
                }
                Error::Timeout(Stage::OffsetManual) => {
                    b"WS53 PASS ADC offset_manual_timeout attempts=2 no_report=1\n"
                }
                Error::Timeout(Stage::Capacitor) => {
                    b"WS53 PASS ADC cap_timeout attempts=2 no_report=1\n"
                }
                Error::StaleStatus(Stage::OffsetAuto) => {
                    b"WS53 PASS ADC stale_status attempts=2 no_report=1\n"
                }
                Error::Capacitor => b"WS53 PASS ADC cap_error attempts=2 no_report=1\n",
                _ => panic!("unexpected calibration failure"),
            }
        }
    };
    assert!(!interrupt::is_enabled(ExternalInterrupt::LSADC));
    let _retained_adc = adc.release_without_powering_off();
    emit(&uart, line);
    timer.borrow_mut().disable(TimerChannel::Channel1);
    emit(&uart, b"WS53 ADC QEMU done\n");
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
