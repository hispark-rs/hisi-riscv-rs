//! QEMU-only RTC control/IRQ fixture; no physical clock/ownership proof.
#![no_std]
#![no_main]
use core::cell::{Cell, RefCell};
use core::num::NonZeroU16;
use critical_section::Mutex;
use embedded_hal::delay::DelayNs;
use hisi_hal::Peripherals;
use hisi_hal::interrupt;
use hisi_hal::peripherals::Uart2;
use hisi_hal::rtc::{self, RtcChannel, RtcDriver, RtcError, RtcMode, RtcTicks};
use hisi_hal::timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks};
use hisi_hal::uart::Uart;
use hisi_riscv_rt::entry;

#[cfg(all(feature = "timeout-smoke", feature = "expiry-race-smoke"))]
compile_error!("build timeout-smoke and expiry-race-smoke as separate RTC fixtures");

#[cfg(feature = "timeout-smoke")]
static SNAPSHOT_CHANNEL: Mutex<Cell<Option<RtcChannel>>> = Mutex::new(Cell::new(None));
#[cfg(feature = "timeout-smoke")]
static OTHER_IRQS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
#[cfg(feature = "timeout-smoke")]
#[unsafe(no_mangle)]
extern "C" fn TIMER_0() {
    unsafe { TimerDriver::acknowledge_interrupt(TimerChannel::Channel0) };
    critical_section::with(|cs| {
        if let Some(channel) = SNAPSHOT_CHANNEL.borrow(cs).get() {
            // Count only interrupts inside source suspension, not the small
            // gaps between assigning the phase marker and calling read_count.
            if !interrupt::is_enabled(channel.interrupt()) {
                let other = match channel {
                    RtcChannel::Channel0 => RtcChannel::Channel1,
                    RtcChannel::Channel1 => RtcChannel::Channel0,
                };
                assert!(interrupt::is_enabled(other.interrupt()));
                assert_eq!(
                    interrupt::priority(channel.interrupt()),
                    interrupt::Priority::P3
                );
                let count = OTHER_IRQS.borrow(cs);
                count.set(count.get() + 1);
            }
        }
    });
}

static EVENTS: Mutex<[Cell<u32>; 2]> = Mutex::new([const { Cell::new(0) }; 2]);
fn record(ch: RtcChannel, index: usize) {
    unsafe {
        rtc::acknowledge_interrupt(ch);
    }
    critical_section::with(|cs| {
        let count = &EVENTS.borrow(cs)[index];
        count.set(count.get() + 1);
    });
}
#[unsafe(no_mangle)]
extern "C" fn RTC_0() {
    record(RtcChannel::Channel0, 0);
}
#[unsafe(no_mangle)]
extern "C" fn RTC_1() {
    record(RtcChannel::Channel1, 1);
}
#[cfg(not(feature = "timeout-smoke"))]
fn counts() -> [u32; 2] {
    critical_section::with(|cs| [EVENTS.borrow(cs)[0].get(), EVENTS.borrow(cs)[1].get()])
}
#[derive(Clone, Copy)]
struct TimerDelay<'a, 'd>(&'a RefCell<TimerDriver<'d>>);
impl DelayNs for TimerDelay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        if ticks == 0 {
            return;
        }
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
    // The named QEMU profile supplies these assumptions, not physical hardware.
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
    let polls = NonZeroU16::new(if cfg!(feature = "timeout-smoke") {
        u16::MAX
    } else {
        256
    })
    .unwrap();
    let mut a = RtcDriver::new(
        unsafe { p.RTC0.assume_application_ownership() },
        delay,
        polls,
    );
    let mut b = RtcDriver::new(
        unsafe { p.RTC1.assume_application_ownership() },
        delay,
        polls,
    );
    emit(&uart, b"WS53 RTC QEMU begin\n");
    assert_eq!(a.read_count(), Err(RtcError::Disabled));
    assert_eq!(b.read_count(), Err(RtcError::Disabled));
    a.start(RtcMode::OneShot, RtcTicks::from_count(32768).unwrap());
    b.start(RtcMode::OneShot, RtcTicks::from_count(32768).unwrap());
    #[cfg(feature = "timeout-smoke")]
    {
        timer.borrow_mut().configure(
            TimerChannel::Channel0,
            TimerMode::Periodic,
            TimerTicks::from_count(3200).unwrap(),
        );
        timer
            .borrow_mut()
            .set_interrupt_mask(TimerChannel::Channel0, false);
        timer.borrow_mut().enable(TimerChannel::Channel0);
        interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
        unsafe {
            interrupt::enable(TimerChannel::Channel0.interrupt());
            interrupt::enable(a.channel().interrupt());
            interrupt::enable(b.channel().interrupt());
            riscv::interrupt::enable();
        }
        interrupt::set_priority(a.channel().interrupt(), interrupt::Priority::P3);
        interrupt::set_priority(b.channel().interrupt(), interrupt::Priority::P3);
        for rtc in [&mut a, &mut b] {
            let before = critical_section::with(|cs| {
                SNAPSHOT_CHANNEL.borrow(cs).set(Some(rtc.channel()));
                OTHER_IRQS.borrow(cs).get()
            });
            assert_eq!(rtc.read_count(), Err(RtcError::SnapshotTimeout));
            critical_section::with(|cs| {
                SNAPSHOT_CHANNEL.borrow(cs).set(None);
                assert!(
                    OTHER_IRQS.borrow(cs).get() > before,
                    "snapshot starved other IRQs"
                );
            });
            assert!(riscv::register::mstatus::read().mie());
            assert!(interrupt::is_enabled(rtc.channel().interrupt()));
            assert_eq!(
                interrupt::priority(rtc.channel().interrupt()),
                interrupt::Priority::P3
            );
        }
        riscv::interrupt::disable();
        timer.borrow_mut().disable(TimerChannel::Channel0);
        drop(a);
        drop(b);
        emit(&uart, b"WS53 PASS RTC timeout channels=2 irq_progress=2\n");
    }
    #[cfg(feature = "expiry-race-smoke")]
    {
        let mut delay = delay;
        a.unmask_interrupt();
        b.unmask_interrupt();
        interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
        unsafe {
            interrupt::enable(a.channel().interrupt());
            interrupt::enable(b.channel().interrupt());
            riscv::interrupt::enable();
        }
        interrupt::set_priority(a.channel().interrupt(), interrupt::Priority::P3);
        interrupt::set_priority(b.channel().interrupt(), interrupt::Priority::P3);
        // Fixture expires each channel between CONTROL read and request write.
        assert_eq!(a.read_count(), Err(RtcError::Expired));
        assert_eq!(b.read_count(), Err(RtcError::Expired));
        assert_eq!(a.read_count(), Err(RtcError::Disabled));
        assert_eq!(b.read_count(), Err(RtcError::Disabled));
        assert!(riscv::register::mstatus::read().mie());
        assert!(interrupt::is_enabled(a.channel().interrupt()));
        assert!(interrupt::is_enabled(b.channel().interrupt()));
        assert_eq!(
            interrupt::priority(a.channel().interrupt()),
            interrupt::Priority::P3
        );
        assert_eq!(
            interrupt::priority(b.channel().interrupt()),
            interrupt::Priority::P3
        );
        let mut budget = 100_000_000;
        while counts() != [1, 1] {
            budget -= 1;
            assert!(budget != 0, "original expiry notification lost");
        }
        delay.delay_ms(2);
        riscv::interrupt::disable();
        assert_eq!(counts(), [1, 1]);
        drop(a);
        drop(b);
        emit(
            &uart,
            b"WS53 PASS RTC expiry race stopped irq_preserved=2\n",
        );
    }
    #[cfg(not(any(feature = "timeout-smoke", feature = "expiry-race-smoke")))]
    {
        let mut delay = delay;
        assert!(!riscv::register::mstatus::read().mie());
        interrupt::set_priority(a.channel().interrupt(), interrupt::Priority::P3);
        interrupt::set_priority(b.channel().interrupt(), interrupt::Priority::P3);
        let first = [a.read_count().unwrap(), b.read_count().unwrap()];
        assert!(!riscv::register::mstatus::read().mie());
        assert!(!interrupt::is_enabled(a.channel().interrupt()));
        assert!(!interrupt::is_enabled(b.channel().interrupt()));
        assert_eq!(
            interrupt::priority(a.channel().interrupt()),
            interrupt::Priority::P3
        );
        assert_eq!(
            interrupt::priority(b.channel().interrupt()),
            interrupt::Priority::P3
        );
        delay.delay_ms(1);
        assert!(a.read_count().unwrap() < first[0]);
        assert!(b.read_count().unwrap() < first[1]);
        a.start(RtcMode::OneShot, RtcTicks::from_count(32).unwrap());
        b.start(RtcMode::Periodic, RtcTicks::from_count(64).unwrap());
        a.unmask_interrupt();
        b.unmask_interrupt();
        interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
        unsafe {
            interrupt::enable(a.channel().interrupt());
            interrupt::enable(b.channel().interrupt());
            riscv::interrupt::enable();
        }
        let mut budget = 100_000_000;
        while counts()[0] < 1 || counts()[1] < 3 {
            budget -= 1;
            assert!(budget != 0, "RTC IRQ missing");
        }
        riscv::interrupt::disable();
        assert_eq!(counts(), [1, 3]);
        assert_eq!(a.read_count(), Err(RtcError::Disabled));
        assert!(b.read_count().is_ok());
        a.stop();
        b.stop();
        delay.delay_ms(5);
        assert!(!a.interrupt_pending() && !b.interrupt_pending());
        assert!(!interrupt::is_enabled(a.channel().interrupt()));
        assert!(!interrupt::is_enabled(b.channel().interrupt()));
        // Cancel a raw event already latched in the local interrupt controller.
        a.start(RtcMode::OneShot, RtcTicks::from_count(32).unwrap());
        delay.delay_ms(2);
        assert!(a.interrupt_pending());
        a.unmask_interrupt();
        unsafe {
            interrupt::enable(a.channel().interrupt());
        }
        a.stop();
        unsafe {
            riscv::interrupt::enable();
        }
        delay.delay_ms(2);
        riscv::interrupt::disable();
        assert_eq!(counts(), [1, 3]);
        // Drop must also quiesce an already pending alarm.
        b.start(RtcMode::OneShot, RtcTicks::from_count(32).unwrap());
        b.unmask_interrupt();
        unsafe {
            interrupt::enable(b.channel().interrupt());
        }
        delay.delay_ms(2);
        assert!(b.interrupt_pending());
        drop(b);
        unsafe {
            riscv::interrupt::enable();
        }
        delay.delay_ms(2);
        riscv::interrupt::disable();
        assert_eq!(counts(), [1, 3]);
        assert!(!interrupt::is_enabled(RtcChannel::Channel1.interrupt()));
        a.start(RtcMode::FreeRunning, RtcTicks::from_count(31).unwrap());
        delay.delay_ms(2);
        assert!(a.read_count().unwrap() > 0xffff0000);
        drop(a);
        emit(
            &uart,
            b"WS53 PASS RTC channels=2 irq=49,50 cancel=1 drop=1\n",
        );
    }
    timer.borrow_mut().disable(TimerChannel::Channel1);
    emit(&uart, b"WS53 RTC QEMU done\n");
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
