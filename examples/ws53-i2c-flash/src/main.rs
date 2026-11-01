//! FlashBoot -> Rust, both real I2C routes, no external responder.
//! Retains the inherited upstream clocks; the shared fixture enables only
//! audited controller gates. No programmer RAM preload or clock setup.
#![no_std]
#![no_main]
use core::{
    num::NonZeroU32,
    ptr::{addr_of, addr_of_mut},
};
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    Peripherals,
    gpio::GpioDriver,
    interrupt,
    peripherals::Uart2,
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::Uart,
    wdt::{Config as WdtConfig, WatchdogDriver, WatchdogMode},
};
use hisi_riscv_rt::entry;
#[path = "../../ws53-flash/src/capture_delay.rs"]
mod capture_delay;
#[allow(dead_code)]
#[path = "../../ws53/ram_smoke/src/i2c_wire.rs"]
mod i2c_wire;

#[unsafe(no_mangle)]
pub static mut WS53_I2C22: [u32; 320] = [0; 320];
#[unsafe(no_mangle)]
pub static mut WS53_I2C22_DATA: u32 = 0x53a022;
#[unsafe(no_mangle)]
pub static mut WS53_I2C22_BSS: u32 = 0;
fn put(i: usize, value: u32) {
    assert!(i < 320);
    unsafe {
        addr_of_mut!(WS53_I2C22)
            .cast::<u32>()
            .add(i)
            .write_volatile(value)
    }
}
fn snapshot() -> [u32; 320] {
    core::array::from_fn(|i| unsafe { addr_of!(WS53_I2C22).cast::<u32>().add(i).read_volatile() })
}
fn fail(code: u32) -> ! {
    put(2, code);
    put(1, 0xee);
    ws53_i2c22_failed()
}
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn ws53_i2c22_failed() -> ! {
    loop {
        unsafe { core::arch::asm!("nop", "nop", options(nomem, nostack)) }
    }
}
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn ws53_i2c22_done() -> ! {
    loop {
        unsafe { core::arch::asm!("nop", options(nomem, nostack)) }
    }
}
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn ws53_i2c22_ready() {
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
}
struct Delay<'a, 'd>(&'a mut TimerDriver<'d>);
impl DelayNs for Delay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        let start = self.0.current_value(TimerChannel::Channel1).unwrap();
        for _ in 0..1_000_000 {
            if start.wrapping_sub(self.0.current_value(TimerChannel::Channel1).unwrap()) >= ticks {
                return;
            }
        }
        fail(30)
    }
}
fn emit(uart: &Uart<'_, Uart2<'_>>, data: &[u8]) {
    for &byte in data {
        let mut n = 1_000_000;
        while uart.try_write_byte(byte).is_err() {
            n -= 1;
            if n == 0 {
                fail(31)
            }
        }
    }
}
fn hex(uart: &Uart<'_, Uart2<'_>>, value: u32) {
    for shift in (0..8).rev() {
        emit(
            uart,
            &[b"0123456789abcdef"[((value >> (shift * 4)) & 15) as usize]],
        );
    }
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    put(0, 0x53f02202);
    put(3, unsafe { addr_of!(WS53_I2C22_DATA).read_volatile() });
    put(4, unsafe { addr_of!(WS53_I2C22_BSS).read_volatile() });
    let memory = unsafe { &*ws53_pac::SystemMemory::PTR };
    for (i, value) in [
        memory.ccore_reset_status().read().bits(),
        memory.bram_share().read().bits(),
        memory.dtcm_share().read().bits(),
        unsafe { &*ws53_pac::Mdma::PTR }
            .enabled_channels()
            .read()
            .bits(),
        memory.platform_enable().read().bits(),
        u32::from(riscv::register::mstatus::read().mie()),
    ]
    .into_iter()
    .enumerate()
    {
        put(5 + i, value)
    }
    if snapshot()[3..11] != [0x53a022, 0, 0, 0x3f8, 0x3c, 0, 0, 0] {
        fail(1)
    }
    let console = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    let mut timer = TimerDriver::new(unsafe { p.TIMER.assume_application_ownership() });
    let ch = TimerChannel::Channel1;
    for irq in [
        ch.interrupt(),
        hisi_hal::interrupt::Interrupt::I2C_0,
        hisi_hal::interrupt::Interrupt::I2C_1,
    ] {
        if interrupt::is_enabled(irq) || interrupt::is_pending(irq) {
            fail(2)
        }
    }
    timer.configure(
        ch,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(ch);
    let watchdog =
        WdtConfig::from_seconds(NonZeroU32::new(32000).unwrap(), 7, WatchdogMode::Reset).unwrap();
    WatchdogDriver::new(unsafe { p.WDT.assume_application_ownership() }, watchdog)
        .stop(&mut Delay(&mut timer))
        .unwrap();
    put(11, unsafe { &*ws53_pac::Wdt::PTR }.wdt_cr().read().bits());
    if snapshot()[11] & 1 != 0 {
        fail(3)
    }
    let _logic =
        unsafe { hisi_hal::control_domain::release_logic_and_clock(&mut Delay(&mut timer)) }
            .unwrap();
    put(12, memory.ccore_reset_status().read().bits());
    if snapshot()[12] != 3 {
        fail(4)
    }
    timer.disable(ch);
    timer.clear_interrupt(ch);
    interrupt::clear_pending(ch.interrupt());
    put(1, 1);
    ws53_i2c22_ready();
    capture_delay::wait(&mut timer);
    emit(&console, b"WS53 I2C22 BEGIN buses=2 phases=16 pad=17\r\n");
    let old = unsafe { &*ws53_pac::MCrg::PTR }.m_clken1().read().bits();
    put(17, u32::from(old));
    let mut bank = GpioDriver::new_gpio0(unsafe { p.GPIO0.assume_application_ownership() });
    for (bus, selected) in [
        i2c_wire::Selection::I2c0(p.I2C0),
        i2c_wire::Selection::I2c1(p.I2C1),
    ]
    .into_iter()
    .enumerate()
    {
        let report = i2c_wire::check_selected(selected, &mut bank, &mut timer, &console);
        for (offset, value) in report.into_iter().enumerate() {
            put(64 + bus * 128 + offset, value);
        }
        for (offset, value) in i2c_wire::boundaries().into_iter().enumerate() {
            put(19 + bus * 16 + offset, value);
        }
        put(13, bus as u32 + 1);
    }
    put(14, u32::from(riscv::register::mstatus::read().mie()));
    put(
        15,
        u32::from(
            unsafe { &*ws53_pac::Timer::PTR }
                .timer_control1()
                .read()
                .enable()
                .bit(),
        ),
    );
    put(
        16,
        u32::from(unsafe { &*ws53_pac::MCrg::PTR }.m_clken1().read().bits()),
    );
    put(18, memory.ccore_reset_status().read().bits());
    assert_eq!(
        snapshot()[14..19],
        [0, 0, u32::from(old), u32::from(old), 3]
    );
    put(1, 5);
    for (i, row) in snapshot().chunks(8).enumerate() {
        emit(&console, b"WS53 I2C22 WORDS row=");
        hex(&console, i as u32);
        for &value in row {
            emit(&console, b" ");
            hex(&console, value);
        }
        emit(&console, b"\r\n");
    }
    emit(&console, b"WS53 I2C22 PASS buses=2 nacks=14 restored=1\r\n");
    for _ in 0..1_000_000 {
        if console.tx_flushed() {
            ws53_i2c22_done()
        }
    }
    fail(33)
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    fail(0xff)
}
