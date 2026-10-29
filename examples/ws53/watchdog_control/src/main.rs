//! QEMU-only WDT control test. Never use this image for a physical RAM takeover:
//! no clock/reset/NMI resource handoff or physical watchdog recovery is proven.
#![no_std]
#![no_main]

use core::num::NonZeroU32;
use embedded_hal::delay::DelayNs;
use hisi_hal::Peripherals;
use hisi_hal::peripherals::Uart2;
use hisi_hal::timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks};
use hisi_hal::uart::Uart;
use hisi_hal::wdt::{Config, WatchdogDriver, WatchdogError, WatchdogMode};
use hisi_riscv_rt::entry;

struct TimerDelay<'a, 'd>(&'a mut TimerDriver<'d>);
impl DelayNs for TimerDelay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        // Uses the established 32 MHz TIMER profile, not a guessed CPU loop rate.
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        if ticks == 0 {
            return;
        }
        let start = self.0.current_value(TimerChannel::Channel1).unwrap();
        for _ in 0..10_000_000 {
            let now = self.0.current_value(TimerChannel::Channel1).unwrap();
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
    // Only the named QEMU fixture supplies these resource/clock assumptions.
    let uart = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    let mut timer = TimerDriver::new(unsafe { p.TIMER.assume_application_ownership() });
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    let mut delay = TimerDelay(&mut timer);
    let config =
        Config::from_seconds(NonZeroU32::new(32000).unwrap(), 64, WatchdogMode::Reset).unwrap();
    let mut wdt = WatchdogDriver::new(unsafe { p.WDT.assume_application_ownership() }, config);
    emit(&uart, b"WS53 WDT QEMU begin\n");
    assert_eq!(wdt.feed(), Err(WatchdogError::Locked));
    assert_eq!(wdt.counter_snapshot(&mut delay), Err(WatchdogError::Locked));
    wdt.start(&mut delay).unwrap();
    #[cfg(feature = "timeout-smoke")]
    {
        // Fixture holds either the clock or snapshot handshake. An Ok(0) would
        // incorrectly turn failure into a valid counter sample and must fail.
        assert_eq!(
            wdt.counter_snapshot(&mut delay),
            Err(WatchdogError::SnapshotTimeout)
        );
        wdt.stop(&mut delay).unwrap();
        emit(&uart, b"WS53 PASS WDT timeout distinct from zero\n");
    }
    #[cfg(not(feature = "timeout-smoke"))]
    {
        let initial = wdt.counter_snapshot(&mut delay).unwrap().ticks();
        assert!(initial > 0 && initial <= (config.load_upper_bits() << 8) | 255);
        delay.delay_ms(2);
        let later = wdt.counter_snapshot(&mut delay).unwrap().ticks();
        assert!(later < initial && later > 0);
        wdt.feed().unwrap();
        assert!(wdt.counter_snapshot(&mut delay).unwrap().ticks() > later);
        assert!(!wdt.interrupt_pending() && !wdt.masked_interrupt_pending());
        wdt.stop(&mut delay).unwrap();
        let stopped = wdt.counter_snapshot(&mut delay).unwrap();
        delay.delay_ms(2);
        assert_eq!(wdt.counter_snapshot(&mut delay).unwrap(), stopped);
        wdt.start(&mut delay).unwrap();
        assert!(wdt.counter_snapshot(&mut delay).unwrap().ticks() > 0);
        wdt.stop(&mut delay).unwrap();
        emit(&uart, b"WS53 PASS WDT count feed stop restart\n");
    }
    delay.0.disable(TimerChannel::Channel1);
    emit(&uart, b"WS53 WDT QEMU done\n");
    loop {
        riscv::asm::wfi();
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    // No flash persistence, reset, or attempt to resume a previous context.
    loop {
        riscv::asm::wfi();
    }
}
