//! Hardware diagnostic of read-only BUSY/CCVR after source-defined commands.
//! QEMU has no BUSY model: do not use its unimplemented read as a zero baseline.
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
    fn until(&mut self, start: u32, us: u32) {
        for _ in 0..10_000_000 {
            if start.wrapping_sub(self.now()) >= us * 32 {
                return;
            }
        }
        panic!("independent TIMER stopped");
    }
}
impl DelayNs for Delay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let start = self.now();
        self.until(start, ns.div_ceil(1000));
    }
}
fn words(uart: &Uart<'_, Uart2<'_>>, values: &[u32]) {
    for value in values {
        for shift in (0..8).rev() {
            super::emit(
                uart,
                &[b"0123456789abcdef"[((value >> (shift * 4)) & 15) as usize]],
            );
        }
    }
    super::emit(uart, b"\r\n");
}
pub fn check(token: Wdt<'_>, timer: &mut TimerDriver<'_>, uart: &Uart<'_, Uart2<'_>>) {
    let regs = unsafe { &*ws53_pac::Wdt::PTR };
    assert!(!regs.wdt_cr().read().wdt_en().bit());
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    let mut delay = Delay(timer);
    let config =
        Config::from_seconds(NonZeroU32::new(32000).unwrap(), 1024, WatchdogMode::Reset).unwrap();
    let mut wdt = WatchdogDriver::new(token, config);
    // kind0 feed,1 stop,2 restart,3 snapshot-only control.
    for settle in [200, 0] {
        for kind in 0..4 {
            for offset in [0, 16, 32, 64, 200] {
                wdt.start(&mut delay).expect("prepare start");
                delay.delay_us(200);
                delay.delay_ms(1);
                let before = wdt
                    .counter_snapshot(&mut delay)
                    .expect("baseline snapshot")
                    .ticks();
                delay.delay_us(settle);
                match kind {
                    0 => wdt.feed().expect("feed"),
                    1 => wdt.stop(&mut delay).expect("stop"),
                    2 => wdt.start(&mut delay).expect("restart"),
                    _ => {}
                }
                let post = [
                    regs.wdt_status().read().bits(),
                    regs.ccvr_status().read().bits(),
                ];
                delay.delay_us(offset);
                let pre = [
                    regs.wdt_status().read().bits(),
                    regs.ccvr_status().read().bits(),
                ];
                let start = delay.now();
                unsafe {
                    regs.ccvr_request()
                        .write_with_zero(|w| w.request().set_bit());
                }
                let mut observations = [[0u32; 4]; 12];
                for (row, us) in observations
                    .iter_mut()
                    .zip([0, 1, 2, 4, 8, 16, 32, 64, 128, 200, 1000, 2100])
                {
                    delay.until(start, us);
                    *row = [
                        us,
                        start.wrapping_sub(delay.now()),
                        regs.wdt_status().read().bits(),
                        regs.ccvr_status().read().bits(),
                    ];
                }
                // Read CNT only once, after the status trace, when validity allows.
                let count = if regs.ccvr_status().read().valid().bit() {
                    regs.wdt_cnt().read().bits()
                } else {
                    u32::MAX
                };
                wdt.stop(&mut delay).expect("cleanup stop");
                delay.delay_us(200);
                assert!(!regs.wdt_cr().read().wdt_en().bit());
                assert!(!wdt.interrupt_pending() && !wdt.masked_interrupt_pending());
                super::emit(uart, b"WS53 WDT HANDSHAKE CASE ");
                words(
                    uart,
                    &[
                        settle, kind, offset, before, post[0], post[1], pre[0], pre[1], count,
                    ],
                );
                for row in observations {
                    super::emit(uart, b"WS53 WDT HANDSHAKE ROW ");
                    words(uart, &row);
                }
            }
        }
    }
    delay.0.disable(TimerChannel::Channel1);
    super::emit(uart, b"WS53 WDT HANDSHAKE OBSERVED cases=40 stopped=1\r\n");
}
