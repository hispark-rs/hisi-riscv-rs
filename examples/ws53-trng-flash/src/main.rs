//! Autonomous vendor FlashBoot -> Rust. Ccore CPU remains reset; no debugger
//! RAM/clock setup. Ring startup and private images, not a live IPC service.
#![no_std]
#![no_main]
use core::{
    num::NonZeroU32,
    ptr::{addr_of, addr_of_mut},
};
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    Peripherals,
    gpio::{GpioBank, GpioDriver, Pad},
    interrupt,
    peripherals::Uart2,
    pinctrl::{Config, PadControl, Pull},
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    trng::TrngDriver,
    uart::Uart,
    wdt::{Config as WdtConfig, WatchdogDriver, WatchdogMode},
};
use hisi_riscv_rt::entry;
#[path = "../../ws53-flash/src/capture_delay.rs"]
mod capture_delay;
#[path = "../../ws53/ram_smoke/src/ipc_random_probe.rs"]
mod ipc_random_probe;

#[unsafe(no_mangle)]
pub static mut WS53_TRNG20: [u32; 64] = [0; 64];
#[unsafe(no_mangle)]
pub static mut WS53_TRNG20_DATA: u32 = 0x53a020;
#[unsafe(no_mangle)]
pub static mut WS53_TRNG20_BSS: u32 = 0;
fn put(i: usize, v: u32) {
    unsafe {
        addr_of_mut!(WS53_TRNG20)
            .cast::<u32>()
            .add(i)
            .write_volatile(v)
    };
}
fn snapshot() -> [u32; 64] {
    core::array::from_fn(|i| unsafe { addr_of!(WS53_TRNG20).cast::<u32>().add(i).read_volatile() })
}
fn fail(code: u32) -> ! {
    put(2, code);
    put(1, 0xee);
    ws53_trng20_failed()
}
#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn ws53_trng20_failed() -> ! {
    loop {
        unsafe { core::arch::asm!("nop", "nop", options(nomem, nostack)) }
    }
}
#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn ws53_trng20_done() -> ! {
    loop {
        unsafe { core::arch::asm!("nop", options(nomem, nostack)) }
    }
}
#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn ws53_trng20_ready() {
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
fn emit(uart: &Uart<'_, Uart2<'_>>, bytes: &[u8]) {
    for &byte in bytes {
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
        )
    }
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    put(0, 0x53f02001);
    put(3, unsafe { addr_of!(WS53_TRNG20_DATA).read_volatile() });
    put(4, unsafe { addr_of!(WS53_TRNG20_BSS).read_volatile() });
    let memory = unsafe { &*ws53_pac::SystemMemory::PTR };
    for (i, v) in [
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
        put(5 + i, v)
    }
    if snapshot()[3..11] != [0x53a020, 0, 0, 0x3f8, 0x3c, 0, 0, 0] {
        fail(1)
    }
    let uart = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    let mut timer = TimerDriver::new(unsafe { p.TIMER.assume_application_ownership() });
    let channel = TimerChannel::Channel1;
    if interrupt::is_enabled(channel.interrupt()) || interrupt::is_pending(channel.interrupt()) {
        fail(2)
    }
    timer.configure(
        channel,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(channel);
    let config =
        WdtConfig::from_seconds(NonZeroU32::new(32000).unwrap(), 7, WatchdogMode::Reset).unwrap();
    WatchdogDriver::new(unsafe { p.WDT.assume_application_ownership() }, config)
        .stop(&mut Delay(&mut timer))
        .unwrap();
    put(11, unsafe { &*ws53_pac::Wdt::PTR }.wdt_cr().read().bits());
    if snapshot()[11] & 1 != 0 {
        fail(3)
    }
    // BootROM/FlashBoot caller is finished, control CPU and its domain were
    // reset by startup. Release logic/clocks only; never start the peer CPU.
    let _logic =
        unsafe { hisi_hal::control_domain::release_logic_and_clock(&mut Delay(&mut timer)) }
            .unwrap();
    put(12, memory.ccore_reset_status().read().bits());
    if snapshot()[12] != 3 {
        fail(4)
    }
    timer.disable(channel);
    timer.clear_interrupt(channel);
    interrupt::clear_pending(channel.interrupt());
    put(1, 1);
    ws53_trng20_ready();
    capture_delay::wait(&mut timer);
    let mut pad = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    let original = pad.configuration().unwrap();
    let gpio = unsafe { &*ws53_pac::Gpio0::PTR };
    if original.mode().mode().code() != 0
        || interrupt::is_enabled(GpioBank::Bank0.interrupt())
        || gpio.gpio_int_en().read().bits() != 0
        || gpio.gpio_sw_ctl().read().bits() != 0
        || gpio.gpio_sw_oen().read().bits() & (1 << 17) == 0
    {
        fail(5)
    }
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
    let pin = pad.into_gpio().unwrap_or_else(|_| fail(6));
    let mut bank = GpioDriver::new_gpio0(unsafe { p.GPIO0.assume_application_ownership() });
    let mut marker = bank.init_output(pin, false).unwrap_or_else(|_| fail(7));
    timer.configure(
        channel,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(channel);
    let ring = unsafe { &*ws53_pac::MCrg::PTR }.trng_ring_control();
    emit(&uart, b"WS53 TRNG20 BEGIN phases=3 pad=17\r\n");
    put(1, 3);
    put(13, u32::from(ring.read().bits()));
    let mut source: Option<TrngDriver<'_>> = None;
    let mut token = Some(unsafe { p.TRNG.assume_application_ownership() });
    for phase in 0usize..3 {
        let before = timer.current_value(channel).unwrap();
        marker.set_high();
        riscv::asm::fence();
        if phase < 2 {
            if phase == 1 {
                token = Some(source.take().unwrap().release_without_stopping())
            }
            let next = unsafe {
                TrngDriver::start_sdk_ring(token.take().unwrap(), &mut Delay(&mut timer))
            }
            .unwrap();
            source = Some(next);
        } else {
            ipc_random_probe::check(source.as_mut().unwrap(), &mut Delay(&mut timer));
        }
        riscv::asm::fence();
        marker.set_low();
        riscv::asm::fence();
        let after = timer.current_value(channel).unwrap();
        for (j, v) in [
            phase as u32,
            before,
            after,
            before.wrapping_sub(after),
            u32::from(ring.read().bits()),
            1,
            0,
            0,
        ]
        .into_iter()
        .enumerate()
        {
            put(24 + 8 * phase + j, v)
        }
        Delay(&mut timer).delay_ms(2);
    }
    let _token = source.take().unwrap().release_without_stopping();
    put(14, u32::from(ring.read().bits()));
    {
        let _released = marker.free();
    }
    let mut restore = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    restore.configure(original).unwrap();
    if restore.configuration().unwrap() != original {
        fail(8)
    }
    put(15, 1);
    timer.disable(channel);
    timer.clear_interrupt(channel);
    interrupt::clear_pending(channel.interrupt());
    put(16, 1);
    put(17, memory.ccore_reset_status().read().bits());
    if snapshot()[17] != 3
        || snapshot()[14] != (snapshot()[13] | 1)
        || riscv::register::mstatus::read().mie()
    {
        fail(9)
    }
    put(18, addr_of!(ipc_random_probe::WS53_IPC_RANDOM_RAM) as u32);
    put(19, core::mem::size_of::<ipc_random_probe::Report>() as u32);
    put(1, 5);
    for (i, row) in snapshot().chunks(8).enumerate() {
        emit(&uart, b"WS53 TRNG20 WORDS row=");
        hex(&uart, i as u32);
        for &word in row {
            emit(&uart, b" ");
            hex(&uart, word)
        }
        emit(&uart, b"\r\n");
    }
    emit(&uart, b"WS53 TRNG20 IMAGE ");
    for i in 0..core::mem::size_of::<ipc_random_probe::Report>() {
        let byte = unsafe {
            addr_of!(ipc_random_probe::WS53_IPC_RANDOM_RAM)
                .cast::<u8>()
                .add(i)
                .read_volatile()
        };
        emit(
            &uart,
            &[
                b"0123456789abcdef"[usize::from(byte >> 4)],
                b"0123456789abcdef"[usize::from(byte & 15)],
            ],
        );
    }
    emit(
        &uart,
        b"\r\nWS53 TRNG20 PASS startups=2 images=13 pad_restore=1\r\n",
    );
    for _ in 0..1_000_000 {
        if uart.tx_flushed() {
            ws53_trng20_done()
        }
    }
    fail(32)
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    fail(0xff)
}
