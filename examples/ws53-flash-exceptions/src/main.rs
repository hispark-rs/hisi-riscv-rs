//! Autonomous vendor FlashBoot -> true exceptions/IRQ53 and GPIO17/D5 markers.
//! No debugger gate, RAM download, Flash command or guest clock initialization.
#![no_std]
#![no_main]
#[cfg(not(all(feature = "flash-autonomous", feature = "irq-exception")))]
compile_error!("Flash exception entry requires flash-autonomous and irq-exception");
#[cfg(any(
    feature = "qemu-only",
    feature = "hil-controlled",
    feature = "legacy-hooks",
    feature = "named-stop",
    feature = "default-stop",
    feature = "default-halt"
))]
compile_error!("Flash exception entry forbids RAM/legacy test environments");

use core::{
    num::NonZeroU32,
    ptr::{addr_of, addr_of_mut},
};
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    gpio::{GpioBank, GpioDriver, Pad},
    interrupt,
    peripherals::Uart2,
    pinctrl::{Config, PadControl, Pull},
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::Uart,
    wdt::{Config as WdtConfig, WatchdogDriver, WatchdogMode},
    Peripherals,
};
use hisi_riscv_rt::entry;
#[path = "../../ws53-flash/src/capture_delay.rs"]
mod capture_delay;
#[path = "../../ws53/exception_context/src/exercise.rs"]
mod exercise;

#[used]
static mut INITIALIZED: [u32; 3] = [0x53ac0123, 0x87654321, 0xfedcba98];
#[used]
static mut ZEROED: [u8; 257] = [0; 257];
#[used]
#[unsafe(no_mangle)]
static mut WS53_FLASH_EXCEPTIONS_PHASE: u32 = 0;
#[used]
#[unsafe(no_mangle)]
static mut WS53_FLASH_EXCEPTIONS_PANIC: u32 = 0;
fn phase(value: u32) {
    unsafe {
        addr_of_mut!(WS53_FLASH_EXCEPTIONS_PHASE).write_volatile(value);
    }
}
fn emit(uart: &Uart<'_, Uart2<'_>>, data: &[u8]) {
    for &byte in data {
        let mut budget = 1_000_000;
        while uart.try_write_byte(byte).is_err() {
            assert!(budget > 0, "UART TX stalled");
            budget -= 1;
        }
    }
}
fn hex(uart: &Uart<'_, Uart2<'_>>, word: u32) {
    for shift in (0..8).rev() {
        emit(
            uart,
            &[b"0123456789abcdef"[((word >> (shift * 4)) & 15) as usize]],
        );
    }
}
struct Delay<'a, 'd>(&'a mut TimerDriver<'d>);
impl DelayNs for Delay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let ticks: u32 = (u64::from(ns) * 32_000_000)
            .div_ceil(1_000_000_000)
            .try_into()
            .unwrap();
        let start = self.0.current_value(TimerChannel::Channel1).unwrap();
        let mut last = start;
        let mut stalled = 0;
        for _ in 0..10_000_000 {
            let now = self.0.current_value(TimerChannel::Channel1).unwrap();
            assert!(last.wrapping_sub(now) <= 3_200_000, "TIMER1 jumped");
            stalled = if now == last { stalled + 1 } else { 0 };
            assert!(stalled < 1_000_000, "TIMER1 stopped");
            last = now;
            if start.wrapping_sub(now) >= ticks {
                return;
            }
        }
        panic!("TIMER1 timeout");
    }
}

#[entry]
fn main() -> ! {
    assert!(!riscv::register::mstatus::read().mie());
    assert_eq!(
        unsafe { addr_of!(INITIALIZED).read_volatile() },
        [0x53ac0123, 0x87654321, 0xfedcba98]
    );
    for i in 0..257 {
        assert_eq!(
            unsafe { addr_of!(ZEROED).cast::<u8>().add(i).read_volatile() },
            0
        );
    }
    assert_eq!(
        unsafe { &*ws53_pac::SystemMemory::PTR }
            .ccore_reset_status()
            .read()
            .bits(),
        0
    );
    let p = Peripherals::take().unwrap();
    let uart = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    let mut timer = TimerDriver::new(unsafe { p.TIMER.assume_application_ownership() });
    let channel = TimerChannel::Channel1;
    assert!(
        !interrupt::is_enabled(channel.interrupt()) && !interrupt::is_pending(channel.interrupt())
    );
    timer.configure(
        channel,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(channel);
    let cfg =
        WdtConfig::from_seconds(NonZeroU32::new(32000).unwrap(), 7, WatchdogMode::Reset).unwrap();
    WatchdogDriver::new(unsafe { p.WDT.assume_application_ownership() }, cfg)
        .stop(&mut Delay(&mut timer))
        .unwrap();
    timer.disable(channel);
    timer.clear_interrupt(channel);
    interrupt::clear_pending(channel.interrupt());
    phase(1);
    capture_delay::wait(&mut timer);
    phase(2);
    let mut pad = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    let original = pad.configuration().unwrap();
    assert_eq!(original.mode().mode().code(), 0);
    assert!(!interrupt::is_enabled(GpioBank::Bank0.interrupt()));
    let regs = unsafe { &*ws53_pac::Gpio0::PTR };
    assert_eq!(regs.gpio_int_en().read().bits(), 0);
    assert_eq!(regs.gpio_sw_ctl().read().bits(), 0);
    assert_ne!(regs.gpio_sw_oen().read().bits() & (1 << 17), 0);
    pad.configure(
        Config::new(
            original.mode(),
            Pad::Mgpio17.drive_setting(0).unwrap(),
            Pull::Down,
            true,
            original.schmitt_enabled(),
        )
        .unwrap(),
    )
    .unwrap();
    let pin = pad
        .into_gpio()
        .unwrap_or_else(|_| panic!("GPIO17 unavailable"));
    let mut bank = GpioDriver::new_gpio0(unsafe { p.GPIO0.assume_application_ownership() });
    let mut output = bank
        .init_output(pin, false)
        .unwrap_or_else(|_| panic!("GPIO bank mismatch"));
    timer.configure(
        channel,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(channel);
    emit(
        &uart,
        b"WS53 FLASH EXCEPTIONS BEGIN clock_hz=32000000 irq=53 pad=17\r\n",
    );
    output.set_high();
    riscv::asm::fence();
    assert!(output.is_set_high());
    Delay(&mut timer).delay_ms(1);
    output.set_low();
    riscv::asm::fence();
    Delay(&mut timer).delay_ms(1);
    output.set_high();
    riscv::asm::fence();
    phase(3);
    exercise::run_probes();
    exercise::irq_probe(&mut timer);
    exercise::finish();
    let state = exercise::snapshot();
    assert_eq!(state[0], 0x530e0001);
    assert_eq!(state[1], 2);
    assert_eq!(state[3], 25);
    assert_eq!(state[4], 0);
    assert_eq!(state[5], 27);
    assert_eq!(state[14], 1);
    assert_eq!(state[31], 0x464c5331);
    output.set_low();
    riscv::asm::fence();
    assert!(!output.is_set_high());
    phase(4);
    Delay(&mut timer).delay_ms(1);
    output.set_high();
    riscv::asm::fence();
    Delay(&mut timer).delay_ms(2);
    output.set_low();
    riscv::asm::fence();
    assert!(!output.is_set_high());
    for (row, chunk) in state.chunks(16).enumerate() {
        emit(&uart, b"WS53 EXCEPTION WORDS row=");
        hex(&uart, row as u32);
        for word in chunk {
            emit(&uart, b" ");
            hex(&uart, *word);
        }
        emit(&uart, b"\r\n");
    }
    let _pin = output.free();
    let mut restored = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    restored.configure(original).unwrap();
    assert_eq!(restored.configuration().unwrap(), original);
    timer.disable(channel);
    timer.clear_interrupt(channel);
    interrupt::clear_pending(channel.interrupt());
    assert!(!riscv::register::mstatus::read().mie());
    assert!(!interrupt::is_enabled(TimerChannel::Channel0.interrupt()));
    emit(
        &uart,
        b"WS53 FLASH EXCEPTIONS PASS returns=25 hooks=27 irq=53 markers=3 pad_restore=1\r\n",
    );
    let mut flushed = false;
    for _ in 0..1_000_000 {
        if uart.tx_flushed() {
            flushed = true;
            break;
        }
    }
    assert!(flushed, "UART flush timeout");
    phase(5);
    exercise::park()
}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    riscv::interrupt::disable();
    unsafe {
        addr_of_mut!(WS53_FLASH_EXCEPTIONS_PANIC)
            .write_volatile(info.location().map_or(1, |p| p.line()));
    }
    exercise::fail()
}
