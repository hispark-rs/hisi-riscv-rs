//! Vendor FlashBoot32MHz -> Embassy Timer/Ticker/queue fixture, single hart.
//! Control CPU remains reset; GPIO17 connects only to DSLogic D5. No DMA/XIP
//! command, peer handshake, epoch reset or clock-tree initialization is used.
#![no_std]
#![no_main]
use core::{
    cell::Cell,
    future::Future,
    num::{NonZeroU16, NonZeroU32},
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};
use critical_section::Mutex;
use embassy_time::{Instant, Timer};
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    Peripherals, embassy,
    gpio::{GpioBank, GpioDriver, Pad},
    interrupt,
    peripherals::Uart2,
    pinctrl::{Config, PadControl, Pull},
    tcxo::TcxoDriver,
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::Uart,
    wdt::{Config as WdtConfig, WatchdogDriver, WatchdogMode},
};
use hisi_riscv_rt::entry;
#[path = "../../ws53-flash/src/capture_delay.rs"]
mod capture_delay;
// Same Timer/Ticker/guard/cancel/16-slot/reentrant-waker assertions as RAM HIL.
// Do not modify this frozen source just to make the Flash path pass.
#[path = "../../ws53/ram_smoke/src/embassy_time_test.rs"]
mod exercise;
#[used]
static mut INITIALIZED: [u32; 3] = [0x53ac0123, 0x87654321, 0xfedcba98];
#[used]
static mut ZEROED: [u8; 257] = [0; 257];
#[used]
#[unsafe(no_mangle)]
static mut WS53_FLASH_EMBASSY_PANIC: u32 = 0;
#[used]
#[unsafe(no_mangle)]
static mut WS53_FLASH_EMBASSY_PHASE: u32 = 0;
fn phase(value: u32) {
    unsafe { core::ptr::addr_of_mut!(WS53_FLASH_EMBASSY_PHASE).write_volatile(value) };
}
fn emit(uart: &Uart<'_, Uart2<'_>>, bytes: &[u8]) {
    for &byte in bytes {
        let mut budget = 1_000_000;
        while uart.try_write_byte(byte).is_err() {
            assert!(budget > 0, "UART TX stalled");
            budget -= 1;
        }
    }
}
fn hex(uart: &Uart<'_, Uart2<'_>>, value: u32) {
    for shift in (0..8).rev() {
        emit(
            uart,
            &[b"0123456789abcdef"[((value >> (4 * shift)) & 15) as usize]],
        );
    }
}
// Used only before the process-lifetime Embassy driver consumes TIMER.
struct Delay<'a, 'd>(&'a mut TimerDriver<'d>);
impl DelayNs for Delay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        let start = self.0.current_value(TimerChannel::Channel1).unwrap();
        let mut last = start;
        let mut stalled = 0;
        for _ in 0..10_000_000 {
            let now = self.0.current_value(TimerChannel::Channel1).unwrap();
            assert!(last.wrapping_sub(now) <= 3_200_000, "setup TIMER1 jumped");
            stalled = if now == last { stalled + 1 } else { 0 };
            assert!(stalled < 1_000_000, "setup TIMER1 stopped");
            last = now;
            if start.wrapping_sub(now) >= ticks {
                return;
            }
        }
        panic!("setup TIMER1 timeout");
    }
}
static WOKEN: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
static WAKES: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static VTABLE: RawWakerVTable = RawWakerVTable::new(|_| raw(), |_| wake(), |_| wake(), |_| {});
fn raw() -> RawWaker {
    RawWaker::new(core::ptr::null(), &VTABLE)
}
fn wake() {
    critical_section::with(|cs| {
        WOKEN.borrow(cs).set(true);
        WAKES.borrow(cs).set(WAKES.borrow(cs).get() + 1);
    });
}
fn wait_ms(ms: u64) {
    let deadline = Instant::now() + embassy_time::Duration::from_millis(ms);
    let mut future = core::pin::pin!(Timer::at(deadline));
    let waker = unsafe { Waker::from_raw(raw()) };
    let mut context = Context::from_waker(&waker);
    for _ in 0..100_000_000 {
        // Register before waiting; an IRQ between poll and loop observation
        // leaves WOKEN set. Never WFI with an unchecked masked/faulted source.
        critical_section::with(|cs| WOKEN.borrow(cs).set(false));
        match future.as_mut().poll(&mut context) {
            Poll::Ready(()) => {
                assert!(Instant::now() >= deadline, "early marker completion");
                return;
            }
            Poll::Pending => {
                let mut budget = 100_000_000;
                while !critical_section::with(|cs| WOKEN.borrow(cs).get()) {
                    budget -= 1;
                    assert_ne!(budget, 0, "marker wake timeout");
                    core::hint::spin_loop();
                }
            }
        }
    }
    panic!("marker poll timeout");
}
#[entry]
fn main() -> ! {
    assert!(!riscv::register::mstatus::read().mie());
    unsafe {
        assert_eq!(
            core::ptr::addr_of!(INITIALIZED).read_volatile(),
            [0x53ac0123, 0x87654321, 0xfedcba98]
        );
        for i in 0..257 {
            assert_eq!(
                (core::ptr::addr_of!(ZEROED) as *const u8)
                    .add(i)
                    .read_volatile(),
                0
            );
        }
    }
    assert_eq!(
        unsafe { &*ws53_pac::SystemMemory::PTR }
            .ccore_reset_status()
            .read()
            .bits(),
        0
    );
    let p = Peripherals::take().unwrap();
    let mut uart = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    let mut timer = TimerDriver::new(unsafe { p.TIMER.assume_application_ownership() });
    let ch = TimerChannel::Channel1;
    assert!(!interrupt::is_enabled(ch.interrupt()) && !interrupt::is_pending(ch.interrupt()));
    timer.configure(
        ch,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(ch);
    let cfg =
        WdtConfig::from_seconds(NonZeroU32::new(32000).unwrap(), 7, WatchdogMode::Reset).unwrap();
    WatchdogDriver::new(unsafe { p.WDT.assume_application_ownership() }, cfg)
        .stop(&mut Delay(&mut timer))
        .unwrap();
    timer.disable(ch);
    timer.clear_interrupt(ch);
    interrupt::clear_pending(ch.interrupt());
    phase(1);
    capture_delay::wait(&mut timer);
    phase(2);
    let mut pad = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    let original = pad.configuration().unwrap();
    assert_eq!(original.mode().mode().code(), 0);
    assert!(!interrupt::is_enabled(GpioBank::Bank0.interrupt()));
    let gpio = unsafe { &*ws53_pac::Gpio0::PTR };
    assert_eq!(gpio.gpio_int_en().read().bits(), 0);
    assert_ne!(gpio.gpio_sw_oen().read().bits() & (1 << 17), 0);
    assert_eq!(gpio.gpio_sw_ctl().read().bits(), 0);
    let config = Config::new(
        original.mode(),
        Pad::Mgpio17.drive_setting(0).unwrap(),
        Pull::Down,
        true,
        original.schmitt_enabled(),
    )
    .unwrap();
    pad.configure(config).unwrap();
    let pin = pad
        .into_gpio()
        .unwrap_or_else(|_| panic!("GPIO17 unavailable"));
    let mut bank = GpioDriver::new_gpio0(unsafe { p.GPIO0.assume_application_ownership() });
    let mut output = bank
        .init_output(pin, false)
        .unwrap_or_else(|_| panic!("wrong GPIO bank"));
    emit(
        &uart,
        b"WS53 FLASH EMBASSY BEGIN clock_hz=32000000 irq=54 pad=17\r\n",
    );
    let clock = TcxoDriver::new(
        unsafe { p.TCXO.assume_application_ownership() },
        NonZeroU16::new(128).unwrap(),
    );
    interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
    phase(3);
    exercise::check(timer, clock, &mut uart);
    assert!(!riscv::register::mstatus::read().mie());
    assert!(embassy::status().initialized && embassy::status().pending_wakers == 0);
    phase(4);
    unsafe { riscv::interrupt::enable() };
    let mut last = Instant::now();
    for ms in [1u32, 2, 4, 5, 10, 20, 8] {
        wait_ms(1);
        let start = Instant::now();
        output.set_high();
        riscv::asm::fence();
        assert!(output.is_set_high());
        wait_ms(u64::from(ms));
        output.set_low();
        riscv::asm::fence();
        assert!(!output.is_set_high());
        let end = Instant::now();
        assert!(
            end >= last
                && end >= start
                && end.duration_since(start).as_micros() >= u64::from(ms) * 1000
        );
        last = end;
        emit(&uart, b"WS53 EMBASSY MARK ms=");
        hex(&uart, ms);
        emit(&uart, b" elapsed_us=");
        hex(
            &uart,
            end.duration_since(start).as_micros().try_into().unwrap(),
        );
        emit(&uart, b"\r\n");
    }
    wait_ms(2); // drain the final task slot normally, no manual IRQ/queue patch
    riscv::interrupt::disable();
    assert_eq!(embassy::status().pending_wakers, 0);
    assert!(!interrupt::is_enabled(ch.interrupt()) && !interrupt::is_pending(ch.interrupt()));
    {
        let _pin = output.free();
    }
    let mut restored = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    restored.configure(original).unwrap();
    assert_eq!(restored.configuration().unwrap(), original);
    let wakes = critical_section::with(|cs| WAKES.borrow(cs).get());
    assert!(wakes >= 15);
    emit(&uart, b"WS53 FLASH EMBASSY PASS timers=5 ticker=3 guards=3 cancel=1 capacity=16 reenter=1 markers=7 pad_restore=1\r\n");
    let flush_deadline = Instant::now() + embassy_time::Duration::from_secs(1);
    let mut flushed = false;
    for _ in 0..1_000_000 {
        if uart.tx_flushed() {
            flushed = true;
            break;
        }
        assert!(
            Instant::now() <= flush_deadline,
            "final UART serializer stopped"
        );
        core::hint::spin_loop();
    }
    assert!(flushed, "final UART flush timeout");
    phase(5);
    loop {
        riscv::asm::wfi();
    }
}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    riscv::interrupt::disable();
    exercise::capture_panic(info);
    unsafe {
        core::ptr::addr_of_mut!(WS53_FLASH_EMBASSY_PANIC)
            .write_volatile(info.location().map_or(1, |p| p.line()));
    }
    loop {
        core::hint::spin_loop();
    }
}
