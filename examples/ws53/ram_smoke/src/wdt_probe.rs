//! Non-expiring WDT test after host stop; no clock/reset/NMI-route changes.
use core::num::NonZeroU32;
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    peripherals::{Uart2, Wdt},
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::Uart,
    wdt::{Config, WatchdogDriver, WatchdogMode},
};
struct Delay<'a, 'd>(&'a mut TimerDriver<'d>);
impl Delay<'_, '_> {
    fn now(&mut self) -> u32 {
        self.0.current_value(TimerChannel::Channel1).unwrap()
    }
}
impl DelayNs for Delay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        let start = self.now();
        for _ in 0..10_000_000 {
            if start.wrapping_sub(self.now()) >= ticks {
                return;
            }
        }
        panic!("independent TIMER stopped");
    }
}
fn hex(uart: &Uart<'_, Uart2<'_>>, value: u32) {
    for shift in (0..8).rev() {
        super::emit(
            uart,
            &[b"0123456789abcdef"[((value >> (shift * 4)) & 15) as usize]],
        );
    }
}
pub fn check(token: Wdt<'_>, timer: &mut TimerDriver<'_>, uart: &Uart<'_, Uart2<'_>>) {
    let regs = unsafe { &*ws53_pac::Wdt::PTR };
    // The host stops the inherited watchdog before loading RAM. Do not take
    // over an enabled, unknown deadline here. QEMU starts disabled as well.
    assert!(!regs.wdt_cr().read().wdt_en().bit());
    assert!(!regs.wdt_raw_intr().read().wdt_raw_intr().bit());
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    let mut delay = Delay(timer);
    // Long source-defined timeout; the experiment stops within tens of ms.
    let config =
        Config::from_seconds(NonZeroU32::new(32000).unwrap(), 1024, WatchdogMode::Reset).unwrap();
    let mut wdt = WatchdogDriver::new(token, config);
    let mut samples = [(0u32, 0u32); 8];
    let epoch = delay.now();
    wdt.start(&mut delay).expect("start failed");
    #[cfg(feature = "wdt-settle-probe")]
    delay.delay_us(200);
    let load = regs.wdt_load().read().bits();
    let control = regs.wdt_cr().read().bits();
    for (index, sample) in samples.iter_mut().enumerate() {
        match index {
            1 | 3 | 5 => delay.delay_ms(10),
            2 => {
                wdt.feed().expect("feed failed");
                #[cfg(feature = "wdt-settle-probe")]
                delay.delay_us(200);
            }
            4 | 7 => {
                wdt.stop(&mut delay).expect("stop failed");
                #[cfg(feature = "wdt-settle-probe")]
                delay.delay_us(200);
            }
            6 => {
                wdt.start(&mut delay).expect("restart failed");
                #[cfg(feature = "wdt-settle-probe")]
                delay.delay_us(200);
            }
            _ => {}
        }
        let count = match wdt.counter_snapshot(&mut delay) {
            Ok(snapshot) => snapshot.ticks(),
            Err(error) => {
                // Preserve the failing handshake before cleanup; a timeout
                // must never be silently converted to a successful zero count.
                let status = regs.ccvr_status().read().bits();
                let stopped = wdt.stop(&mut delay).is_ok();
                super::emit(uart, b"WS53 WDT SNAPSHOT FAIL ");
                hex(uart, index as u32);
                hex(
                    uart,
                    match error {
                        hisi_hal::wdt::WatchdogError::Locked => 1,
                        hisi_hal::wdt::WatchdogError::UnlockTimeout => 2,
                        hisi_hal::wdt::WatchdogError::SnapshotTimeout => 3,
                    },
                );
                hex(uart, status);
                hex(uart, u32::from(stopped));
                super::emit(uart, b"\r\n");
                for (phase, &(ticks, count)) in samples[..index].iter().enumerate() {
                    super::emit(uart, b"WS53 WDT PARTIAL ");
                    hex(uart, phase as u32);
                    hex(uart, ticks);
                    hex(uart, count);
                    super::emit(uart, b"\r\n");
                }
                delay.0.disable(TimerChannel::Channel1);
                return;
            }
        };
        *sample = (epoch.wrapping_sub(delay.now()), count);
    }
    // Already stopped before printing or checking samples. Hardware host always
    // resets afterwards, including on panic/timeout, never resumes old context.
    assert!(!regs.wdt_cr().read().wdt_en().bit());
    assert!(!wdt.interrupt_pending() && !wdt.masked_interrupt_pending());
    #[cfg(feature = "wdt-settle-probe")]
    super::emit(uart, b"WS53 WDT SETTLE 000000c8\r\n");
    super::emit(uart, b"WS53 WDT CONFIG ");
    hex(uart, load);
    hex(uart, control);
    super::emit(uart, b"\r\n");
    for (ticks, count) in samples {
        super::emit(uart, b"WS53 WDT SAMPLE ");
        hex(uart, ticks);
        hex(uart, count);
        super::emit(uart, b"\r\n");
    }
    delay.0.disable(TimerChannel::Channel1);
    super::emit(
        uart,
        b"WS53 PASS wdt samples=8 feed=1 stop=2 restart=1 pending=0\r\n",
    );
}
