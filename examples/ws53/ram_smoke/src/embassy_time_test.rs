//! Same-ELF Embassy Timer/Ticker tests. No pad or counter-epoch writes.
use core::{
    cell::Cell,
    future::Future,
    pin::Pin,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};
use critical_section::Mutex;
use embassy_time::{Duration, Instant, Ticker, Timer};
use hisi_hal::{
    embassy, interrupt,
    peripherals::{Timer as TimerToken, Uart2},
    tcxo::{TcxoDriver, TcxoError},
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::Uart,
};

static WAKES: Mutex<[Cell<u32>; 24]> = Mutex::new([const { Cell::new(0) }; 24]);
static IRQS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static REENTER: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
static VTABLE: RawWakerVTable = RawWakerVTable::new(raw, wake, wake, |_| {});
fn raw(data: *const ()) -> RawWaker {
    RawWaker::new(data, &VTABLE)
}
fn waker(id: usize) -> Waker {
    assert!(id < 24);
    // Opaque integer IDs, never dereferenced; only single-hart CS-protected data.
    unsafe { Waker::from_raw(raw(id as *const ())) }
}
fn count(id: usize) -> u32 {
    critical_section::with(|cs| WAKES.borrow(cs)[id].get())
}
fn wake(data: *const ()) {
    let id = data as usize;
    let reenter = critical_section::with(|cs| {
        let v = &WAKES.borrow(cs)[id];
        v.set(v.get() + 1);
        id == 20 && REENTER.borrow(cs).replace(false)
    });
    if reenter {
        // A waker may call the time driver again. No RefCell borrow or CS may
        // surround user callbacks; the newly queued wake must remain live.
        embassy_time_driver::schedule_wake(Instant::now().as_ticks() + 2_000, &waker(21));
    }
}
#[unsafe(no_mangle)]
extern "C" fn TIMER_1() {
    critical_section::with(|cs| IRQS.borrow(cs).set(IRQS.borrow(cs).get() + 1));
    embassy::on_alarm_interrupt();
}
fn wait_wake(id: usize, previous: u32) {
    let mut budget = 100_000_000;
    while count(id) == previous {
        budget -= 1;
        assert_ne!(budget, 0, "Embassy timer did not wake");
        core::hint::spin_loop();
    }
}
fn run<F: Future>(future: F, id: usize) -> F::Output {
    let w = waker(id);
    let mut cx = Context::from_waker(&w);
    let mut future = core::pin::pin!(future);
    loop {
        let before = count(id);
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => wait_wake(id, before),
        }
    }
}
fn poll<F: Future>(future: Pin<&mut F>, id: usize) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(&waker(id)))
}
fn ch0() -> (u32, u32) {
    let r = unsafe { &*TimerToken::ptr() };
    (
        r.timer_load_count0().read().bits(),
        r.timer_control0().read().bits(),
    )
}
pub fn check(
    mut timer: TimerDriver<'static>,
    mut clock: TcxoDriver<'static>,
    uart: &mut Uart<'_, Uart2<'_>>,
) {
    assert_eq!(Instant::now().as_ticks(), 0);
    assert!(!embassy::status().initialized);
    embassy_time_driver::schedule_wake(1, &waker(23));
    let ch = TimerChannel::Channel1;
    let peer = ch0();
    clock.enable().unwrap(); // Preserves the inherited epoch, no clear.
    timer.configure(
        ch,
        TimerMode::OneShot,
        TimerTicks::from_micros(1_000_000).unwrap(),
    );
    timer.enable(ch);
    (timer, clock) = match unsafe { embassy::init(timer, clock) } {
        Err((embassy::InitError::TimerRunning, t, c)) => (t, c),
        _ => panic!("running guard"),
    };
    timer.disable(ch);
    unsafe { interrupt::enable(ch.interrupt()) };
    (timer, clock) = match unsafe { embassy::init(timer, clock) } {
        Err((embassy::InitError::TimerInterruptActive, t, c)) => (t, c),
        _ => panic!("IRQ guard"),
    };
    unsafe { interrupt::disable(ch.interrupt()) };
    timer.clear_interrupt(ch);
    {
        let _snapshot = clock.begin_snapshot().unwrap();
    }
    (timer, clock) = match unsafe { embassy::init(timer, clock) } {
        Err((embassy::InitError::Clock(TcxoError::SnapshotBusy), t, c)) => (t, c),
        _ => panic!("snapshot guard"),
    };
    let mut budget = 100_000;
    loop {
        match clock.try_discard_snapshot() {
            Ok(()) => break,
            Err(nb::Error::WouldBlock) => {
                budget -= 1;
                assert_ne!(budget, 0);
            }
            Err(_) => panic!("snapshot drain"),
        }
    }
    assert!(unsafe { embassy::init(timer, clock) }.is_ok());
    assert!(embassy::status().initialized);
    unsafe { riscv::interrupt::enable() };
    wait_wake(23, 0);
    let mut previous = Instant::now();
    for us in [0, 1, 100, 1000, 5000] {
        let deadline = Instant::now() + Duration::from_micros(us);
        run(Timer::at(deadline), 0);
        let now = Instant::now();
        assert!(now >= deadline && now >= previous, "early/backwards time");
        previous = now;
    }
    // The earlier timer must wake while the later future is still Pending.
    let later = Instant::now() + Duration::from_millis(100);
    let mut late = core::pin::pin!(Timer::at(later));
    assert!(poll(late.as_mut(), 1).is_pending());
    run(Timer::after_millis(2), 2);
    assert!(Instant::now() < later);
    assert!(poll(late.as_mut(), 1).is_pending());
    run(late, 1);
    assert!(Instant::now() >= later);
    let mut ticker = Ticker::every(Duration::from_millis(1));
    for _ in 0..3 {
        run(ticker.next(), 3);
    }
    assert!(
        run(
            embassy_time::with_timeout(Duration::from_millis(1), core::future::pending::<()>()),
            3
        )
        .is_err()
    );
    // Generic Embassy Timer has no deregistration on drop. Its bounded retained
    // waker expires normally; cancellation must not poison the next timer.
    {
        let mut cancelled = core::pin::pin!(Timer::after_millis(1));
        assert!(poll(cancelled.as_mut(), 4).is_pending());
    }
    run(Timer::after_millis(3), 5);
    assert!(count(4) > 0);
    assert_eq!(embassy::status().pending_wakers, 0);
    // Spurious ISR invocation must not complete a not-yet-expired Timer.
    riscv::interrupt::disable();
    {
        let mut future = core::pin::pin!(Timer::after_millis(10));
        assert!(poll(future.as_mut(), 6).is_pending());
        embassy::on_alarm_interrupt();
        assert!(poll(future.as_mut(), 6).is_pending());
    }
    unsafe { riscv::interrupt::enable() };
    run(Timer::after_millis(15), 5);
    // Fill all16 slots with distinct wakers, then force one eviction. Early
    // wake on overflow is legal; the real Timer tests above recheck deadlines.
    let counts: [u32; 17] = core::array::from_fn(count);
    let deadline = Instant::now().as_ticks() + 100_000;
    for id in 0..17 {
        embassy_time_driver::schedule_wake(deadline, &waker(id));
    }
    assert_eq!(embassy::status().pending_wakers, 16);
    assert_eq!((0..17).filter(|&id| count(id) != counts[id]).count(), 1);
    for (id, old) in counts.into_iter().enumerate() {
        wait_wake(id, old);
    }
    assert_eq!(embassy::status().pending_wakers, 0);
    // u64::MAX is retained as a real deadline; it cannot overflow the32-bit
    // alarm. Same-waker replacement moves it earlier and releases its slot.
    embassy_time_driver::schedule_wake(u64::MAX, &waker(19));
    let r = unsafe { &*TimerToken::ptr() };
    // A pending TCXO snapshot can temporarily choose a1ms retry segment.
    let load = r.timer_load_count1().read().bits();
    assert!(load == u32::MAX || load == (32_000 | 15));
    let before = count(19);
    embassy_time_driver::schedule_wake(Instant::now().as_ticks() + 1000, &waker(19));
    wait_wake(19, before);
    critical_section::with(|cs| REENTER.borrow(cs).set(true));
    let before = count(21);
    embassy_time_driver::schedule_wake(Instant::now().as_ticks() + 1000, &waker(20));
    wait_wake(21, before);
    run(Timer::after_millis(1), 22);
    riscv::interrupt::disable();
    assert_eq!(embassy::status().pending_wakers, 0);
    assert!(Instant::now() >= previous);
    assert_eq!(ch0(), peer);
    assert!(!interrupt::is_enabled(ch.interrupt()));
    assert!(!interrupt::is_pending(ch.interrupt()));
    assert!(critical_section::with(|cs| IRQS.borrow(cs).get()) > 0);
    #[cfg(feature = "embassy-fault-probe")]
    faults(uart);
    super::emit(uart, b"WS53 PASS embassy irq=54 timers=5 ticker=3 guards=3 cancel=1 capacity=16 reenter=1 monotonic=1\r\n");
}
#[unsafe(no_mangle)]
static mut WS53_EMBASSY_PANIC: u32 = 0;
pub fn capture_panic(info: &core::panic::PanicInfo<'_>) {
    if let Some(location) = info.location() {
        unsafe {
            core::ptr::addr_of_mut!(WS53_EMBASSY_PANIC).write_volatile(location.line());
        }
    }
}

#[cfg(feature = "embassy-fault-probe")]
fn faults(uart: &mut Uart<'_, Uart2<'_>>) {
    for (stop, restore) in [
        (
            b"WS53 EMBASSY STALL\r\n".as_slice(),
            b"WS53 EMBASSY RECOVER SNAPSHOT\r\n".as_slice(),
        ),
        (
            b"WS53 EMBASSY CLOCK OFF\r\n".as_slice(),
            b"WS53 EMBASSY RECOVER CLOCK\r\n".as_slice(),
        ),
    ] {
        riscv::interrupt::disable();
        gate(uart, stop);
        for _ in 0..32 {
            let _ = Instant::now();
        }
        let held = Instant::now().as_ticks();
        let old = count(0);
        embassy_time_driver::schedule_wake(held + 1000, &waker(0));
        let first_irq = critical_section::with(|cs| IRQS.borrow(cs).get());
        unsafe { riscv::interrupt::enable() };
        let mut budget = 100_000_000;
        while critical_section::with(|cs| IRQS.borrow(cs).get()) - first_irq < 3 {
            budget -= 1;
            assert_ne!(budget, 0, "stalled clock prevented retry IRQ");
        }
        assert_eq!(Instant::now().as_ticks(), held);
        assert_eq!(
            embassy::status().clock,
            embassy::ClockStatus::SnapshotPending
        );
        assert_eq!(count(0), old, "clock fault completed deadline early");
        assert_eq!(embassy::status().pending_wakers, 1);
        gate(uart, restore);
        wait_wake(0, old);
        assert!(Instant::now().as_ticks() >= held + 1000);
        assert_eq!(embassy::status().pending_wakers, 0);
    }
    riscv::interrupt::disable();
    super::emit(
        uart,
        b"WS53 PASS embassy_fault frozen=2 retry=2 recover=2 no_early=1\r\n",
    );
}
#[cfg(feature = "embassy-fault-probe")]
fn gate(uart: &mut Uart<'_, Uart2<'_>>, marker: &[u8]) {
    super::emit(uart, marker);
    let mut budget = 100_000_000;
    loop {
        match uart.read_byte() {
            Ok(b'g') => return,
            Err(nb::Error::WouldBlock) => {}
            _ => panic!("fault host input"),
        }
        budget -= 1;
        assert_ne!(budget, 0, "fault host timeout");
    }
}
