//! Shared HIL/QEMU snapshot fixture, no counter clear or pad writes.
//! Requires exclusive application snapshot-window access, stopped vendor/core
//! users, no NMI/DMA access, no sleep/rate change, and the 32MHz clock profile.
use core::cell::Cell;
use core::num::NonZeroU32;
use critical_section::Mutex;
use hisi_hal::interrupt;
use hisi_hal::peripherals::Uart2;
use hisi_hal::tcxo::TcxoDriver;
use hisi_hal::timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks};
use hisi_hal::uart::Uart;

const CHANNEL: TimerChannel = TimerChannel::Channel1;
static NOTIFICATIONS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
// A halted debugger reads this RAM object using its ELF symbol. It is never
// used for synchronization or aliased while running; this is not MMIO.
#[unsafe(no_mangle)]
static mut WS53_TCXO_SAMPLES: [u64; 6] = [0; 6];

pub fn check(tcxo: &mut TcxoDriver<'_>, timer: &mut TimerDriver<'_>, uart: &Uart<'_, Uart2<'_>>) {
    tcxo.enable().unwrap(); // Keep the inherited epoch; never call clear_counter here.
    let frequency = NonZeroU32::new(32_000_000).unwrap();
    for (index, us) in [10_000, 20_000, 40_000].into_iter().enumerate() {
        timer.configure(
            CHANNEL,
            TimerMode::OneShot,
            TimerTicks::from_micros(us).unwrap(),
        );
        let before = tcxo.read_counter().unwrap_or_else(|_| super::fail());
        unsafe {
            interrupt::enable(CHANNEL.interrupt());
            riscv::interrupt::enable();
        }
        timer.enable(CHANNEL);
        let mut budget = 100_000_000u32;
        while critical_section::with(|cs| NOTIFICATIONS.borrow(cs).get()) != index as u32 + 1 {
            if budget == 0 {
                super::fail();
            }
            budget -= 1;
            core::hint::spin_loop();
        }
        riscv::interrupt::disable();
        unsafe {
            interrupt::disable(CHANNEL.interrupt());
        }
        timer.disable(CHANNEL);
        timer.clear_interrupt(CHANNEL);
        let after = tcxo.read_counter().unwrap_or_else(|_| super::fail());
        unsafe {
            let samples = core::ptr::addr_of_mut!(WS53_TCXO_SAMPLES).cast::<u64>();
            samples.add(index * 2).write_volatile(before.ticks());
            samples.add(index * 2 + 1).write_volatile(after.ticks());
        }
        let elapsed = after
            .checked_duration_since(before, frequency)
            .unwrap_or_else(|| super::fail());
        let expected = u128::from(us);
        // This is a frequency/ordering smoke test, not a calibrated timebase.
        // Includes snapshot, IRQ-to-main and stop/ack overhead on both hosts.
        if elapsed.as_micros() < expected * 95 / 100 || elapsed.as_micros() > expected * 105 / 100 {
            super::emit(uart, b"WS53 FAIL tcxo interval\r\n");
            super::fail();
        }
    }
    let first = tcxo.read_counter().unwrap_or_else(|_| super::fail());
    let mut last = first;
    for _ in 0..32 {
        let next = tcxo.read_counter().unwrap_or_else(|_| super::fail());
        if next.ticks() < last.ticks() {
            super::fail();
        }
        last = next;
    }
    if last.ticks() == first.ticks() {
        super::fail();
    }
    super::emit(
        uart,
        b"WS53 PASS tcxo samples=6 monotonic=32 timer1_irq=54 count=3 no_clear=1\r\n",
    );
}

#[unsafe(no_mangle)]
extern "C" fn TIMER_1() {
    #[cfg(feature = "timer-async-smoke")]
    if super::timer_async::dispatching() {
        super::timer_async::record(CHANNEL);
        return;
    }
    unsafe {
        TimerDriver::acknowledge_interrupt(CHANNEL);
    }
    critical_section::with(|cs| {
        let value = NOTIFICATIONS.borrow(cs);
        value.set(value.get() + 1);
    });
}

#[cfg(feature = "tcxo-nonblocking-smoke")]
pub fn check_nonblocking(tcxo: &mut TcxoDriver<'_>, uart: &Uart<'_, Uart2<'_>>) {
    use hisi_hal::tcxo::{CounterSnapshot, CounterTicks, TcxoError};
    fn finish(snapshot: &mut CounterSnapshot<'_, '_>) -> CounterTicks {
        let mut budget = 1_000_000;
        loop {
            match snapshot.try_read() {
                Ok(value) => return value,
                Err(nb::Error::WouldBlock) => (),
                Err(nb::Error::Other(_)) => super::fail(),
            }
            budget -= 1;
            assert_ne!(budget, 0, "nonblocking TCXO snapshot stalled");
        }
    }
    let first = tcxo.read_counter().unwrap();
    let mut last = first;
    for _ in 0..32 {
        let mut snapshot = tcxo.begin_snapshot().unwrap();
        let value = finish(&mut snapshot);
        assert!(value.ticks() >= last.ticks());
        // A completed transaction returns the same sample without a new refresh.
        assert_eq!(snapshot.try_read(), Ok(value));
        last = value;
    }
    for _ in 0..8 {
        // Drop before reading. Hardware may already be ready, but no result was
        // consumed: enable/clear must refuse until explicit drain or next begin.
        let _ = tcxo.begin_snapshot().unwrap();
        assert_eq!(tcxo.enable(), Err(TcxoError::SnapshotBusy));
        assert_eq!(tcxo.clear_counter(), Err(TcxoError::SnapshotBusy));
        let mut budget = 1_000_000;
        loop {
            match tcxo.try_discard_snapshot() {
                Ok(()) => break,
                Err(nb::Error::WouldBlock) => (),
                Err(nb::Error::Other(_)) => super::fail(),
            }
            budget -= 1;
            assert_ne!(budget, 0, "cancelled TCXO snapshot stalled");
        }
        let mut snapshot = tcxo.begin_snapshot().unwrap();
        let value = finish(&mut snapshot);
        assert!(value.ticks() >= last.ticks());
        last = value;
    }
    assert!(last.ticks() > first.ticks());
    tcxo.try_discard_snapshot().unwrap();
    super::emit(
        uart,
        b"WS53 PASS tcxo_nonblocking samples=40 cached=32 drops=8 guards=16 no_clear=1\r\n",
    );
}
