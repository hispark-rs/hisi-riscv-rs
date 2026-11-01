//! GPIO17-only FlashBoot fixture. Vendor FlashBoot supplies UART2/TIMER clocks;
//! the runtime validates its memory handoff and leaves the control CPU reset.
//! All users of GPIO0 are quiescent. MIO17 is connected only to DSLogic D5.
//! SW_CTL is observed, never interpreted or written. No debugger setup is used.
#![no_std]
#![no_main]

use core::num::NonZeroU32;
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    Peripherals,
    gpio::{GpioBank, GpioDriver, Pad},
    interrupt,
    peripherals::Uart2,
    pinctrl::{Config, PadControl, Pull},
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::Uart,
    wdt::{Config as WdtConfig, WatchdogDriver, WatchdogMode},
};
use hisi_riscv_rt::entry;

// Reuse the proven autonomous delay without changing its frozen source.
#[path = "../../ws53-flash/src/capture_delay.rs"]
mod capture_delay;
mod gpio_async;

#[used]
static mut INITIALIZED: [u32; 3] = [0x53ac0123, 0x87654321, 0xfedcba98];
#[used]
static mut ZEROED: [u8; 257] = [0; 257];
#[used]
#[unsafe(no_mangle)]
static mut WS53_FLASH_GPIO_PANIC: u32 = 0;

fn fail() -> ! {
    panic!("Flash GPIO fixture failed")
}
fn emit(uart: &Uart<'_, Uart2<'_>>, data: &[u8]) {
    for &byte in data {
        let mut budget = 1_000_000;
        while uart.try_write_byte(byte).is_err() {
            assert!(budget > 0, "UART stuck");
            budget -= 1;
        }
    }
}
fn number2(uart: &Uart<'_, Uart2<'_>>, value: u32) {
    assert!(value < 100);
    emit(
        uart,
        &[b'0' + (value / 10) as u8, b'0' + (value % 10) as u8],
    );
}
fn hex(uart: &Uart<'_, Uart2<'_>>, value: u32) {
    for shift in (0..8).rev() {
        emit(
            uart,
            &[b"0123456789abcdef"[((value >> (shift * 4)) & 15) as usize]],
        );
    }
}
fn source() -> u32 {
    unsafe { &*ws53_pac::Gpio0::PTR }
        .gpio_sw_ctl()
        .read()
        .bits()
}
fn peer() -> [u32; 7] {
    let gpio = unsafe { &*ws53_pac::Gpio0::PTR };
    [
        gpio.gpio_sw_oen().read().bits() & !(1 << 17),
        gpio.gpio_int_mask().read().bits() & !(1 << 17),
        gpio.gpio_int_en().read().bits() & !(1 << 17),
        gpio.gpio_int_type().read().bits() & !(1 << 17),
        gpio.gpio_int_polarity().read().bits() & !(1 << 17),
        gpio.gpio_int_dedge().read().bits() & !(1 << 17),
        gpio.gpio_int_debounce().read().bits() & !(1 << 17),
    ]
}

struct Delay<'a, 'd>(&'a mut TimerDriver<'d>);
impl DelayNs for Delay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        assert!(!interrupt::is_enabled(TimerChannel::Channel1.interrupt()));
        let regs = unsafe { &*ws53_pac::Timer::PTR };
        assert!(regs.timer_control1().read().enable().bit());
        let required = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        let start = self.0.current_value(TimerChannel::Channel1).unwrap();
        let mut last = start;
        let mut stalled = 0;
        for _ in 0..10_000_000 {
            assert!(
                regs.timer_control1().read().enable().bit(),
                "TIMER1 disabled"
            );
            let now = self.0.current_value(TimerChannel::Channel1).unwrap();
            // A single bounded sample cannot consume 100 ms in this fixture.
            // Reject discontinuities instead of calling them completed delays.
            assert!(last.wrapping_sub(now) <= 3_200_000, "TIMER1 jumped");
            stalled = if now == last { stalled + 1 } else { 0 };
            assert!(stalled < 1_000_000, "TIMER1 stalled");
            last = now;
            if start.wrapping_sub(now) >= required {
                return;
            }
        }
        panic!("TIMER1 stopped");
    }
}

fn timer_idle() {
    let regs = unsafe { &*ws53_pac::Timer::PTR };
    assert!(!regs.timer_control1().read().enable().bit());
    assert_eq!(regs.timer_raw_intr1().read().bits(), 0);
    assert!(!interrupt::is_enabled(TimerChannel::Channel1.interrupt()));
    assert!(!interrupt::is_pending(TimerChannel::Channel1.interrupt()));
    assert_eq!(
        unsafe { &*ws53_pac::SystemMemory::PTR }
            .ccore_reset_status()
            .read()
            .bits(),
        0
    );
}

fn preamble(bank: &mut GpioDriver<'_>, timer: &mut TimerDriver<'_>) -> Config {
    let mut owner = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    let original = owner.configuration().unwrap();
    assert_eq!(
        original.mode().mode().code(),
        0,
        "pad still routed to another peripheral"
    );
    owner.configure(gpio_async::cfg(Pull::Down)).unwrap();
    let pin = owner.into_gpio().unwrap_or_else(|_| fail());
    let mut output = bank.init_output(pin, false).unwrap_or_else(|_| fail());
    Delay(timer).delay_ms(2);
    for width in [1, 2, 4] {
        output.set_high();
        assert!(output.is_set_high());
        Delay(timer).delay_ms(width);
        output.set_low();
        assert!(!output.is_set_high());
        Delay(timer).delay_ms(1);
    }
    Delay(timer).delay_ms(1);
    // Return the exclusive pin token before claiming the same pad for input.
    // Pull-down remains active, preventing a floating interval at direction change.
    let _ = output.free();
    original
}

#[entry]
fn main() -> ! {
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
    assert!(!riscv::register::mstatus::read().mie());
    let p = Peripherals::take().unwrap();
    let uart = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    let mut timer = TimerDriver::new(unsafe { p.TIMER.assume_application_ownership() });
    timer_idle();
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    let config =
        WdtConfig::from_seconds(NonZeroU32::new(32000).unwrap(), 7, WatchdogMode::Reset).unwrap();
    let mut wdt = WatchdogDriver::new(unsafe { p.WDT.assume_application_ownership() }, config);
    wdt.stop(&mut Delay(&mut timer)).unwrap();
    assert!(
        !unsafe { &*ws53_pac::Wdt::PTR }
            .wdt_cr()
            .read()
            .wdt_en()
            .bit()
    );
    timer.disable(TimerChannel::Channel1);
    capture_delay::wait(&mut timer);
    emit(&uart, b"WS53 FLASH observation delay_ms=30000 early_checks=retained\r\nWS53 FLASH data/bss OK\r\nWS53 FLASH watchdog stopped\r\n");

    // Refuse another bank IRQ owner before any GPIO mutation. No blanket EOI.
    assert!(!interrupt::is_enabled(GpioBank::Bank0.interrupt()));
    assert!(!interrupt::is_pending(GpioBank::Bank0.interrupt()));
    if unsafe { &*ws53_pac::Gpio0::PTR }
        .gpio_int_en()
        .read()
        .bits()
        != 0
    {
        panic!("GPIO0 interrupt owner");
    }
    assert_ne!(
        unsafe { &*ws53_pac::Gpio0::PTR }
            .gpio_sw_oen()
            .read()
            .bits()
            & (1 << 17),
        0,
        "old GPIO output still driving"
    );
    let before_source = source();
    let before_peer = peer();
    emit(
        &uart,
        b"WS53 FLASH GPIO BEGIN pad=17 irq=34\r\nWS53 FLASH GPIO SOURCE before=",
    );
    hex(&uart, before_source);
    emit(&uart, b"\r\nWS53 FLASH GPIO PREAMBLE BEGIN\r\n");
    interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
    let mut bank = GpioDriver::new_gpio0(unsafe { p.GPIO0.assume_application_ownership() });
    timer_idle();
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    let original = preamble(&mut bank, &mut timer);
    assert_eq!(source(), before_source);
    assert_eq!(peer(), before_peer);
    emit(&uart, b"WS53 FLASH GPIO PREAMBLE END\r\n");
    let steps = gpio_async::check(&mut bank, &mut timer, &uart, original);
    assert_eq!(source(), before_source);
    assert_eq!(peer(), before_peer);
    timer.disable(TimerChannel::Channel1);
    timer.clear_interrupt(TimerChannel::Channel1);
    interrupt::clear_pending(TimerChannel::Channel1.interrupt());
    assert!(!interrupt::is_enabled(TimerChannel::Channel1.interrupt()));
    assert!(!interrupt::is_pending(TimerChannel::Channel1.interrupt()));
    timer_idle();
    emit(&uart, b"WS53 FLASH GPIO SOURCE after=");
    hex(&uart, source());
    emit(&uart, b"\r\nWS53 FLASH GPIO PASS irq=34 wakes=8 steps=");
    number2(&uart, steps);
    emit(&uart, b" pad_config_restore=1\r\n");
    loop {
        riscv::asm::wfi();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    riscv::interrupt::disable();
    unsafe {
        core::ptr::addr_of_mut!(WS53_FLASH_GPIO_PANIC)
            .write_volatile(info.location().map_or(1, |p| p.line()));
    }
    loop {
        core::hint::spin_loop();
    }
}
