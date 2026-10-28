//! Two application RTCs under bounded RAM handoff; independent TIMER1 delay.
//! No common RTC bank, clock mux, pad, ROM or low-power service is modified.
use core::cell::{Cell, RefCell};
use core::num::NonZeroU16;
use critical_section::Mutex;
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    interrupt,
    peripherals::{Rtc0, Rtc1, Uart2},
    rtc::{self, RtcChannel, RtcDriver, RtcError, RtcMode, RtcTicks},
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::Uart,
};

static EVENTS: Mutex<[Cell<u32>; 2]> = Mutex::new([const { Cell::new(0) }; 2]);
static STATUS: Mutex<Cell<[[u32; 8]; 2]>> = Mutex::new(Cell::new([[0; 8]; 2]));
fn raw(ch: RtcChannel) -> u32 {
    match ch {
        RtcChannel::Channel0 => unsafe { &*ws53_pac::Rtc0::PTR }.raw_intr().read().bits(),
        RtcChannel::Channel1 => unsafe { &*ws53_pac::Rtc1::PTR }.raw_intr().read().bits(),
    }
}
fn record(ch: RtcChannel, i: usize) {
    unsafe {
        interrupt::disable(ch.interrupt());
    }
    let before = raw(ch);
    unsafe {
        rtc::acknowledge_interrupt(ch);
    }
    let after = raw(ch);
    let count = critical_section::with(|cs| {
        let c = &EVENTS.borrow(cs)[i];
        if c.get() < 8 {
            let mut status = STATUS.borrow(cs).get();
            status[i][c.get() as usize] = (before << 16) | after;
            STATUS.borrow(cs).set(status);
        }
        c.set(c.get().saturating_add(1));
        c.get()
    });
    if count >= 8 {
        unsafe {
            interrupt::disable(ch.interrupt());
        }
    }
}
#[unsafe(no_mangle)]
extern "C" fn RTC_0() {
    record(RtcChannel::Channel0, 0);
}
#[unsafe(no_mangle)]
extern "C" fn RTC_1() {
    record(RtcChannel::Channel1, 1);
}
fn count(i: usize) -> u32 {
    critical_section::with(|cs| EVENTS.borrow(cs)[i].get())
}

#[derive(Clone, Copy)]
struct Delay<'a, 'd>(&'a RefCell<&'a mut TimerDriver<'d>>);
impl Delay<'_, '_> {
    fn counter(&self) -> u32 {
        self.0
            .borrow_mut()
            .current_value(TimerChannel::Channel1)
            .unwrap()
    }
}
impl DelayNs for Delay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        let start = self.counter();
        for _ in 0..10_000_000 {
            let now = self.counter();
            if start.wrapping_sub(now) >= ticks {
                return;
            }
        }
        panic!("TIMER1 stopped");
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
// ACK crosses a clock domain. Do not return directly into the same asserted
// source or busy-wait in its ISR. Progress in normal context with a bounded
// delay; only restore delivery after raw status has deasserted.
fn service<D: DelayNs>(
    rtc: &mut RtcDriver<'_, D>,
    mut delay: Delay<'_, '_>,
    ms: u32,
    index: usize,
    observed: &mut u32,
    observer: &mut impl FnMut(usize, u32),
) {
    // Bound the *whole* observation window with TIMER1. Counting 32us delays
    // also accumulates GPIO, MMIO and uncached-Flash instruction overhead; on
    // silicon that made a requested 53ms window span ten RTC periods.
    let start = delay.counter();
    let budget = ms.checked_mul(32_000).unwrap();
    for _ in 0..=ms * 32 {
        if start.wrapping_sub(delay.counter()) >= budget {
            return;
        }
        delay.delay_us(32);
        let current = count(index);
        if current != *observed {
            assert_eq!(current, *observed + 1, "RTC observation skipped an event");
            *observed = current;
            observer(index, current);
        }
        if !interrupt::is_enabled(rtc.channel().interrupt()) && !rtc.interrupt_pending() {
            interrupt::clear_pending(rtc.channel().interrupt());
            unsafe {
                interrupt::enable(rtc.channel().interrupt());
            }
        }
    }
    panic!("RTC observation timer did not progress");
}
#[allow(dead_code)] // Original RAM entry; Flash also observes the IRQs on a pad.
pub fn check(a: Rtc0<'_>, b: Rtc1<'_>, timer: &mut TimerDriver<'_>, uart: &Uart<'_, Uart2<'_>>) {
    check_with_observer(a, b, timer, uart, |_, _| {});
}
pub fn check_with_observer(
    a: Rtc0<'_>,
    b: Rtc1<'_>,
    timer: &mut TimerDriver<'_>,
    uart: &Uart<'_, Uart2<'_>>,
    mut observer: impl FnMut(usize, u32),
) -> bool {
    assert!(!riscv::register::mstatus::read().mie());
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    let timer = RefCell::new(timer);
    let mut delay = Delay(&timer);
    let polls = NonZeroU16::new(4096).unwrap();
    let mut a = RtcDriver::new(a, delay, polls);
    let mut b = RtcDriver::new(b, delay, polls);
    interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
    let mut good = true;
    for (i, rtc) in [&mut a, &mut b].into_iter().enumerate() {
        let mut observed = 0;
        super::emit(uart, b"WS53 RTC BEGIN ");
        hex(uart, i as u32);
        super::emit(uart, b"\r\n");
        rtc.start(RtcMode::OneShot, RtcTicks::from_count(32768).unwrap());
        let first = rtc.read_count().unwrap();
        delay.delay_ms(120);
        let delta = first.wrapping_sub(rtc.read_count().unwrap());
        super::emit(uart, b"WS53 RTC SAMPLE ");
        hex(uart, delta);
        super::emit(uart, b"\r\n");
        good &= (3800..=3900).contains(&delta);
        rtc.stop();
        good &= rtc.read_count() == Err(RtcError::Disabled);
        // Peripheral mask retains raw status, even with local delivery enabled.
        rtc.start(RtcMode::OneShot, RtcTicks::from_count(327).unwrap());
        unsafe {
            interrupt::enable(rtc.channel().interrupt());
            riscv::interrupt::enable();
        }
        delay.delay_ms(15);
        super::emit(uart, b"WS53 RTC MASK ");
        hex(uart, count(i));
        hex(uart, raw(rtc.channel()));
        super::emit(uart, b"\r\n");
        good &= count(i) == 0 && rtc.interrupt_pending();
        rtc.unmask_interrupt();
        service(rtc, delay, 2, i, &mut observed, &mut observer);
        super::emit(uart, b"WS53 RTC UNMASK ");
        hex(uart, count(i));
        hex(uart, raw(rtc.channel()));
        super::emit(uart, b"\r\n");
        good &= count(i) == 1 && !rtc.interrupt_pending();
        rtc.stop();
        delay.delay_ms(20);
        good &= count(i) == 1;
        rtc.start(RtcMode::Periodic, RtcTicks::from_count(655).unwrap());
        rtc.unmask_interrupt();
        unsafe {
            interrupt::enable(rtc.channel().interrupt());
        }
        service(rtc, delay, 53, i, &mut observed, &mut observer);
        rtc.stop();
        good &= count(i) == 3;
        delay.delay_ms(30);
        good &= count(i) == 3 && !rtc.interrupt_pending();
        rtc.start(RtcMode::OneShot, RtcTicks::from_count(327).unwrap());
        rtc.unmask_interrupt();
        unsafe {
            interrupt::enable(rtc.channel().interrupt());
        }
        service(rtc, delay, 15, i, &mut observed, &mut observer);
        rtc.stop();
        riscv::interrupt::disable();
        good &= count(i) == 4 && observed == 4 && rtc.read_count() == Err(RtcError::Disabled);
        super::emit(uart, b"WS53 RTC EVENTS ");
        hex(uart, count(i));
        super::emit(uart, b"\r\n");
        let status = critical_section::with(|cs| STATUS.borrow(cs).get()[i]);
        super::emit(uart, b"WS53 RTC ACK ");
        for value in status {
            hex(uart, value);
        }
        super::emit(uart, b"\r\n");
    }
    drop(a);
    drop(b);
    timer.borrow_mut().disable(TimerChannel::Channel1);
    if good {
        super::emit(
            uart,
            b"WS53 PASS rtc channels=2 irq=49,50 mask=1 periodic=1 rearm=1 stop=1\r\n",
        );
    } else {
        super::emit(uart, b"WS53 FAIL rtc observations\r\n");
    }
    good
}
