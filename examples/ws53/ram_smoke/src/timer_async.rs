//! Async one-shots on both channels; TCXO observes lower delay bounds without
//! clearing its inherited epoch. Only re-poll a Pending future after its wake.
use core::{
    cell::Cell,
    future::Future,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};
use critical_section::Mutex;
use embedded_hal_async::delay::DelayNs;
use hisi_hal::{
    interrupt,
    peripherals::{Timer, Uart2},
    tcxo::TcxoDriver,
    timer::{AsyncBindError, TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::Uart,
};
static DISPATCH: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
static WAKE: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
static COUNTS: Mutex<[Cell<u32>; 2]> = Mutex::new([const { Cell::new(0) }; 2]);
static VTABLE: RawWakerVTable = RawWakerVTable::new(|_| raw(), |_| wake(), |_| wake(), |_| {});
fn raw() -> RawWaker {
    RawWaker::new(core::ptr::null(), &VTABLE)
}
fn wake() {
    critical_section::with(|cs| WAKE.borrow(cs).set(true));
}
pub fn dispatching() -> bool {
    critical_section::with(|cs| DISPATCH.borrow(cs).get())
}
fn count(ch: TimerChannel) -> u32 {
    critical_section::with(|cs| COUNTS.borrow(cs)[ch.index()].get())
}
pub fn record(ch: TimerChannel) {
    assert!(
        unsafe { TimerDriver::on_async_interrupt(ch) },
        "unexpected hardware timer notification"
    );
    critical_section::with(|cs| {
        COUNTS.borrow(cs)[ch.index()].set(COUNTS.borrow(cs)[ch.index()].get() + 1)
    });
}
#[cfg(not(feature = "tcxo-snapshot-smoke"))]
#[unsafe(no_mangle)]
extern "C" fn TIMER_1() {
    record(TimerChannel::Channel1);
}
fn run<F: Future>(future: F, cx: &mut Context<'_>, _tcxo: &mut TcxoDriver<'_>) -> F::Output {
    let mut future = core::pin::pin!(future);
    // Flash has already qualified this independent counter. RAM retains its
    // historical polling policy; neither profile relies on WFI or host input.
    #[cfg(feature = "tcxo-snapshot-smoke")]
    let start = now(_tcxo);
    for _ in 0..1024 {
        critical_section::with(|cs| WAKE.borrow(cs).set(false));
        if let Poll::Ready(value) = future.as_mut().poll(cx) {
            return value;
        }
        let mut budget = 100_000_000;
        while !critical_section::with(|cs| WAKE.borrow(cs).get()) {
            #[cfg(feature = "tcxo-snapshot-smoke")]
            assert!(now(_tcxo) - start < 640_000, "timer future wake deadline");
            budget -= 1;
            assert_ne!(budget, 0, "timer future did not wake");
            core::hint::spin_loop();
        }
    }
    panic!("timer future exceeded poll budget");
}
fn now(tcxo: &mut TcxoDriver<'_>) -> u64 {
    tcxo.read_counter().unwrap().ticks()
}
fn dwell(tcxo: &mut TcxoDriver<'_>, us: u32) {
    let start = now(tcxo);
    let mut budget = 10_000_000;
    while now(tcxo) - start < u64::from(us) * 32 {
        budget -= 1;
        assert_ne!(budget, 0, "TCXO did not progress");
    }
}
fn controls(ch: TimerChannel) -> (u32, u32) {
    let r = unsafe { &*Timer::ptr() };
    match ch {
        TimerChannel::Channel0 => (
            r.timer_load_count0().read().bits(),
            r.timer_control0().read().bits(),
        ),
        TimerChannel::Channel1 => (
            r.timer_load_count1().read().bits(),
            r.timer_control1().read().bits(),
        ),
    }
}
/// Observable intervals long enough for a digital capture. Sub-microsecond
/// waits are verified by the actual Future/IRQ and TCXO assertions separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Wait1ms,
    Wait3ms,
    Wait5ms,
    CancelBeforeExpiry,
    ReuseBeforeExpiry,
    CancelAfterExpiry,
    ReuseAfterExpiry,
}
#[allow(dead_code)] // The RAM entry deliberately uses a no-op observer.
impl Stage {
    pub fn index(self) -> u8 {
        self as u8
    }
    pub fn minimum_us(self) -> u32 {
        match self {
            Self::Wait1ms | Self::ReuseBeforeExpiry | Self::ReuseAfterExpiry => 1000,
            Self::Wait3ms => 3000,
            Self::Wait5ms => 5000,
            Self::CancelBeforeExpiry => 20_000,
            // Let 1ms latch with MIE disabled, cancel, then observe 5ms quiet.
            Self::CancelAfterExpiry => 6000,
        }
    }
}
#[allow(dead_code)] // Flash validates the final handoff independently.
pub fn counts() -> [u32; 2] {
    [count(TimerChannel::Channel0), count(TimerChannel::Channel1)]
}

#[allow(dead_code)] // Flash adds observation without replacing the RAM entry.
pub fn check(timer: &mut TimerDriver<'_>, tcxo: &mut TcxoDriver<'_>, uart: &Uart<'_, Uart2<'_>>) {
    check_with_observer(timer, tcxo, uart, |_, _, _| {});
}

pub fn check_with_observer(
    timer: &mut TimerDriver<'_>,
    tcxo: &mut TcxoDriver<'_>,
    uart: &Uart<'_, Uart2<'_>>,
    mut observe: impl FnMut(TimerChannel, Stage, bool),
) {
    tcxo.enable().unwrap();
    critical_section::with(|cs| DISPATCH.borrow(cs).set(true));
    let waker = unsafe { Waker::from_raw(raw()) };
    let mut cx = Context::from_waker(&waker);
    for ch in [TimerChannel::Channel0, TimerChannel::Channel1] {
        let peer = if ch == TimerChannel::Channel0 {
            TimerChannel::Channel1
        } else {
            TimerChannel::Channel0
        };
        let peer_before = controls(peer);
        riscv::interrupt::disable();
        unsafe { interrupt::disable(ch.interrupt()) };
        timer.configure(
            ch,
            TimerMode::OneShot,
            TimerTicks::from_micros(100_000).unwrap(),
        );
        sample_load(ch, 0);
        timer.enable(ch);
        sample_load(ch, 1);
        assert!(matches!(
            unsafe { timer.async_delay(ch) },
            Err(AsyncBindError::Running)
        ));
        timer.disable(ch);
        sample_load(ch, 2);
        unsafe { interrupt::enable(ch.interrupt()) };
        assert!(matches!(
            unsafe { timer.async_delay(ch) },
            Err(AsyncBindError::InterruptsEnabled)
        ));
        unsafe { interrupt::disable(ch.interrupt()) };
        timer.configure(
            ch,
            TimerMode::OneShot,
            TimerTicks::from_micros(100).unwrap(),
        );
        timer.enable(ch);
        let start = now(tcxo);
        let mut budget = 1_000_000;
        while !timer.interrupt_pending(ch) {
            assert!(now(tcxo) - start < 320_000, "guard expiry timeout");
            budget -= 1;
            assert_ne!(budget, 0, "guard expiry stopped clocks");
        }
        sample_load(ch, 3);
        timer.disable(ch);
        sample_load(ch, 4);
        assert!(matches!(
            unsafe { timer.async_delay(ch) },
            Err(AsyncBindError::PendingInterrupt)
        ));
        timer.clear_interrupt(ch);
        {
            let mut delay = unsafe { timer.async_delay(ch) }.unwrap();
            unsafe { riscv::interrupt::enable() };
            run(delay.delay_ns(0), &mut cx, tcxo);
            assert_eq!(count(ch), 0);
            for ns in [1, 32, 1000] {
                let start = now(tcxo);
                run(delay.delay_ns(ns), &mut cx, tcxo);
                assert!((now(tcxo) - start) * 1_000_000_000 >= u64::from(ns) * 32_000_000);
            }
            observe(ch, Stage::Wait1ms, true);
            let start = now(tcxo);
            run(delay.delay_us(1000), &mut cx, tcxo);
            assert!(now(tcxo) - start >= 32_000);
            observe(ch, Stage::Wait1ms, false);
            observe(ch, Stage::Wait3ms, true);
            let start = now(tcxo);
            run(delay.delay_ms(3), &mut cx, tcxo);
            assert!(now(tcxo) - start >= 96_000);
            observe(ch, Stage::Wait3ms, false);
            observe(ch, Stage::Wait5ms, true);
            let start = now(tcxo);
            run(
                delay.delay_ticks(TimerTicks::from_micros(5000).unwrap()),
                &mut cx,
                tcxo,
            );
            assert!(now(tcxo) - start >= 160_000);
            assert_eq!(count(ch), 6);
            observe(ch, Stage::Wait5ms, false);
            // Cancellation before expiry; an unrelated/spurious ISR cannot complete it.
            observe(ch, Stage::CancelBeforeExpiry, true);
            riscv::interrupt::disable();
            critical_section::with(|cs| WAKE.borrow(cs).set(false));
            {
                let mut future = core::pin::pin!(delay.delay_ms(10));
                assert!(future.as_mut().poll(&mut cx).is_pending());
                assert!(!unsafe { TimerDriver::on_async_interrupt(ch) });
                assert!(future.as_mut().poll(&mut cx).is_pending());
            }
            assert!(
                !interrupt::is_enabled(ch.interrupt()) && !interrupt::is_pending(ch.interrupt())
            );
            unsafe { riscv::interrupt::enable() };
            dwell(tcxo, 20_000);
            assert_eq!(count(ch), 6);
            assert!(!critical_section::with(|cs| WAKE.borrow(cs).get()));
            observe(ch, Stage::CancelBeforeExpiry, false);
            observe(ch, Stage::ReuseBeforeExpiry, true);
            run(delay.delay_ms(1), &mut cx, tcxo);
            assert_eq!(count(ch), 7);
            observe(ch, Stage::ReuseBeforeExpiry, false);
            // Let expiry latch while global delivery is disabled, then cancel.
            observe(ch, Stage::CancelAfterExpiry, true);
            riscv::interrupt::disable();
            critical_section::with(|cs| WAKE.borrow(cs).set(false));
            {
                let mut future = core::pin::pin!(delay.delay_ms(1));
                assert!(future.as_mut().poll(&mut cx).is_pending());
                let start = now(tcxo);
                let mut budget = 1_000_000;
                while !interrupt::is_pending(ch.interrupt()) {
                    assert!(now(tcxo) - start < 320_000, "cancel expiry timeout");
                    budget -= 1;
                    assert_ne!(budget, 0, "cancel expiry stopped clocks");
                }
            }
            assert!(
                !interrupt::is_enabled(ch.interrupt()) && !interrupt::is_pending(ch.interrupt())
            );
            unsafe { riscv::interrupt::enable() };
            dwell(tcxo, 5000);
            assert_eq!(count(ch), 7);
            assert!(!critical_section::with(|cs| WAKE.borrow(cs).get()));
            observe(ch, Stage::CancelAfterExpiry, false);
            observe(ch, Stage::ReuseAfterExpiry, true);
            run(delay.delay_us(1000), &mut cx, tcxo);
            assert_eq!(count(ch), 8);
            observe(ch, Stage::ReuseAfterExpiry, false);
        }
        assert!(!timer.interrupt_pending(ch));
        assert!(!interrupt::is_enabled(ch.interrupt()));
        sample_load(ch, 5); // Observe LOAD readback; do not assume retained reload value.
        probe_load(ch);
        assert_eq!(controls(ch).1 & 0xf, 8); // stopped, one-shot, masked
        assert_eq!(controls(peer), peer_before);
    }
    riscv::interrupt::disable();
    critical_section::with(|cs| DISPATCH.borrow(cs).set(false));
    super::emit(
        uart,
        b"WS53 PASS timer_async channels=2 expiries=16 cancels=4 guards=6\r\n",
    );
}

#[unsafe(no_mangle)]
static mut WS53_TIMER_ASYNC_PANIC: u32 = 0;
pub fn capture_panic(info: &core::panic::PanicInfo<'_>) {
    if let Some(location) = info.location() {
        unsafe {
            core::ptr::addr_of_mut!(WS53_TIMER_ASYNC_PANIC).write_volatile(location.line());
        }
    }
}

#[unsafe(no_mangle)]
static mut WS53_TIMER_ASYNC_LOADS: [u32; 12] = [0; 12];
fn sample_load(ch: TimerChannel, phase: usize) {
    unsafe {
        core::ptr::addr_of_mut!(WS53_TIMER_ASYNC_LOADS)
            .cast::<u32>()
            .add(ch.index() * 6 + phase)
            .write_volatile(controls(ch).0);
    }
}

const LOAD_CASES: [u32; 16] = [
    0, 1, 2, 7, 15, 16, 17, 31, 32, 33, 0xff, 0x100, 0x12345670, 0x89abcdef, 0xfffffff0, 0xffffffff,
];
#[unsafe(no_mangle)]
static mut WS53_TIMER_ASYNC_LOAD_PROBE: [u32; 32] = [0; 32];
fn write_load(ch: TimerChannel, value: u32) {
    let r = unsafe { &*Timer::ptr() };
    match ch {
        TimerChannel::Channel0 => unsafe {
            r.timer_load_count0()
                .write_with_zero(|w| w.load_count().bits(value));
        },
        TimerChannel::Channel1 => unsafe {
            r.timer_load_count1()
                .write_with_zero(|w| w.load_count().bits(value));
        },
    }
}
fn probe_load(ch: TimerChannel) {
    // Driver and IRQ are stopped; only observe reload readback. Never start
    // arbitrary periods, never replay a read value as configuration.
    for (i, value) in LOAD_CASES.into_iter().enumerate() {
        write_load(ch, value);
        let read = controls(ch).0;
        assert_eq!(read, controls(ch).0);
        assert_eq!(read & !15, value & !15);
        unsafe {
            core::ptr::addr_of_mut!(WS53_TIMER_ASYNC_LOAD_PROBE)
                .cast::<u32>()
                .add(ch.index() * 16 + i)
                .write_volatile(read);
        }
    }
    write_load(ch, 32_000);
}
