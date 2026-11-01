//! Autonomous FlashBoot -> dedicated ITCM relocation, UART/TIMER/D5 observation.
//! Ccore stays reset. No Flash-controller, cache configuration or PMP writes.
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
    uart::Uart,
    wdt::{Config as WdtConfig, WatchdogDriver, WatchdogMode},
};
use hisi_riscv_rt::entry;
#[path = "../../ws53-flash/src/capture_delay.rs"]
mod capture_delay;

#[used]
#[unsafe(no_mangle)]
static mut WS53_RAM19: [u32; 64] = [0; 64];
static mut INITIALIZED: u32 = 0x53a019;
static mut ZEROED: u32 = 0;

#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".ws53.ramrodata.table")]
static WS53_RAM19_TABLE: [u32; 4] = [0x12, 0x34, 0x56, 0x78];

#[inline(never)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".ws53.ramtext.leaf")]
unsafe extern "C" fn ws53_ram19_leaf(seed: u32) -> u32 {
    let value = unsafe {
        addr_of!(WS53_RAM19_TABLE)
            .cast::<u32>()
            .add((seed & 3) as usize)
            .read_volatile()
    };
    seed.wrapping_mul(33).wrapping_add(value)
}

#[inline(never)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".ws53.ramtext.root")]
unsafe extern "C" fn ws53_ram19_root(seed: u32) -> u32 {
    unsafe { ws53_ram19_leaf(seed) }.rotate_left(7) ^ seed
}

#[inline(never)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".ws53.ramtext.run")]
unsafe extern "C" fn ws53_ram19_run(count: u32, mut seed: u32) -> u32 {
    for _ in 0..count {
        seed = unsafe { ws53_ram19_root(seed) };
    }
    seed
}

#[inline(never)]
#[unsafe(no_mangle)]
extern "C" fn ws53_ram19_ready() {
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
}

#[inline(never)]
#[unsafe(no_mangle)]
extern "C" fn ws53_ram19_done() {
    unsafe { core::arch::asm!("nop", options(nomem, nostack)) }
}

#[inline(never)]
#[unsafe(no_mangle)]
extern "C" fn ws53_ram19_failed() {
    unsafe { core::arch::asm!("nop", "nop", options(nomem, nostack)) }
}

unsafe extern "C" {
    static __ws53_ramtext_load: u8;
    static __ws53_ramtext_begin: u8;
    static __ws53_ramtext_end: u8;
    static __stack_start__: u32;
    static __stack_top__: u32;
    fn trap_vector();
}
fn put(i: usize, v: u32) {
    unsafe {
        addr_of_mut!(WS53_RAM19)
            .cast::<u32>()
            .add(i)
            .write_volatile(v)
    }
}
fn snapshot() -> [u32; 64] {
    core::array::from_fn(|i| unsafe { addr_of!(WS53_RAM19).cast::<u32>().add(i).read_volatile() })
}
fn fail(code: u32) -> ! {
    put(2, code);
    put(1, 0xee);
    ws53_ram19_failed();
    loop {
        core::hint::spin_loop()
    }
}
fn copied() -> bool {
    let source = addr_of!(__ws53_ramtext_load);
    let dest = addr_of!(__ws53_ramtext_begin);
    let size = addr_of!(__ws53_ramtext_end) as usize - dest as usize;
    (0..size).all(|i| unsafe { source.add(i).read_volatile() == dest.add(i).read_volatile() })
}
fn cache_state() -> [u32; 3] {
    let (i, d, p): (u32, u32, u32);
    unsafe {
        core::arch::asm!("csrr {i}, 0x7c0", "csrr {d}, 0x7c1", "csrr {p}, 0x7c6",
        i=out(reg)i, d=out(reg)d, p=out(reg)p, options(nostack))
    };
    [i, d, p]
}
fn emit(uart: &Uart<'_, Uart2<'_>>, bytes: &[u8]) {
    for &b in bytes {
        let mut budget = 1_000_000;
        while uart.try_write_byte(b).is_err() {
            if budget == 0 {
                fail(81)
            }
            budget -= 1;
        }
    }
}
fn hex(uart: &Uart<'_, Uart2<'_>>, word: u32) {
    for shift in (0..8).rev() {
        emit(
            uart,
            &b"0123456789abcdef"[((word >> (shift * 4)) & 15) as usize..][..1],
        );
    }
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
        fail(82)
    }
}
fn expected(count: u32, mut seed: u32) -> u32 {
    for _ in 0..count {
        seed = seed
            .wrapping_mul(33)
            .wrapping_add([0x12, 0x34, 0x56, 0x78][(seed & 3) as usize])
            .rotate_left(7)
            ^ seed;
    }
    seed
}

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    put(0, 0x53f01901);
    put(
        3,
        (addr_of!(__ws53_ramtext_end) as usize - addr_of!(__ws53_ramtext_begin) as usize) as u32,
    );
    put(4, addr_of!(__ws53_ramtext_begin) as u32);
    put(5, addr_of!(__ws53_ramtext_load) as u32);
    put(6, addr_of!(__ws53_ramtext_end) as u32);
    put(7, addr_of!(WS53_RAM19_TABLE) as u32);
    put(8, ws53_ram19_run as *const () as u32);
    put(9, unsafe { addr_of!(INITIALIZED).read_volatile() });
    put(10, unsafe { addr_of!(ZEROED).read_volatile() });
    put(11, u32::from(copied()));
    let memory = unsafe { &*ws53_pac::SystemMemory::PTR };
    put(14, memory.ccore_reset_status().read().bits());
    put(15, memory.bram_share().read().bits());
    put(16, memory.dtcm_share().read().bits());
    put(
        17,
        unsafe { &*ws53_pac::Mdma::PTR }
            .enabled_channels()
            .read()
            .bits(),
    );
    put(18, memory.platform_enable().read().bits());
    for (i, v) in cache_state().into_iter().enumerate() {
        put(19 + i, v);
    }
    let sp: u32;
    unsafe { core::arch::asm!("mv {sp}, sp", sp=out(reg)sp, options(nostack)) };
    put(22, sp);
    put(23, riscv::register::mtvec::read().bits() as u32);
    put(27, u32::from(riscv::register::mstatus::read().mie()));
    put(29, 32_000_000);
    put(30, 3);
    put(56, addr_of!(__stack_start__) as u32);
    put(57, addr_of!(__stack_top__) as u32);
    put(61, ws53_ram19_leaf as *const () as u32);
    put(62, ws53_ram19_root as *const () as u32);
    let state = snapshot();
    if state[9..12] != [0x53a019, 0, 1]
        || state[14..20] != [0, 0x3f8, 0x3c, 0, 0, 6]
        || state[20] != 0
        || state[27] != 0
    {
        fail(1);
    }
    if !(state[56]..state[57]).contains(&sp)
        || sp & 15 != 0
        || state[23] & !3 != trap_vector as *const () as u32
    {
        fail(2)
    }
    let uart = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    let mut timer = TimerDriver::new(unsafe { p.TIMER.assume_application_ownership() });
    let channel = TimerChannel::Channel1;
    if interrupt::is_enabled(channel.interrupt()) || interrupt::is_pending(channel.interrupt()) {
        fail(3)
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
    put(58, unsafe { &*ws53_pac::Wdt::PTR }.wdt_cr().read().bits());
    if snapshot()[58] & 1 != 0 {
        fail(4)
    }
    timer.disable(channel);
    timer.clear_interrupt(channel);
    interrupt::clear_pending(channel.interrupt());
    put(1, 1);
    ws53_ram19_ready();
    capture_delay::wait(&mut timer);
    let mut pad = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    let original = pad.configuration().unwrap();
    let regs = unsafe { &*ws53_pac::Gpio0::PTR };
    if original.mode().mode().code() != 0
        || interrupt::is_enabled(GpioBank::Bank0.interrupt())
        || regs.gpio_int_en().read().bits() != 0
        || regs.gpio_sw_ctl().read().bits() != 0
        || regs.gpio_sw_oen().read().bits() & (1 << 17) == 0
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
    let mut output = bank.init_output(pin, false).unwrap_or_else(|_| fail(7));
    timer.configure(
        channel,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(channel);
    emit(&uart, b"WS53 RAM19 BEGIN samples=3 pad=17\r\n");
    put(1, 3);
    for (index, n) in [1024, 4096, 16384].into_iter().enumerate() {
        let seed = state[9] ^ (index as u32);
        let before = timer.current_value(channel).unwrap();
        output.set_high();
        riscv::asm::fence();
        let result = unsafe { ws53_ram19_run(n, seed) };
        riscv::asm::fence();
        output.set_low();
        let after = timer.current_value(channel).unwrap();
        let expected = expected(n, seed);
        for (offset, v) in [
            n,
            seed,
            result,
            expected,
            before,
            after,
            state[8],
            index as u32,
        ]
        .into_iter()
        .enumerate()
        {
            put(32 + 8 * index + offset, v);
        }
        if result != expected {
            fail(13)
        }
        Delay(&mut timer).delay_ms(2);
    }
    let _pin = output.free();
    let mut restored = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    restored.configure(original).unwrap();
    if restored.configuration().unwrap() != original {
        fail(14)
    }
    put(12, 1);
    timer.disable(channel);
    timer.clear_interrupt(channel);
    interrupt::clear_pending(channel.interrupt());
    put(13, 1);
    for (i, v) in cache_state().into_iter().enumerate() {
        put(24 + i, v);
    }
    put(28, u32::from(copied()));
    if snapshot()[19..22] != snapshot()[24..27]
        || snapshot()[28] != 1
        || riscv::register::mstatus::read().mie()
    {
        fail(15)
    }
    put(1, 5);
    for (index, row) in snapshot().chunks(8).enumerate() {
        emit(&uart, b"WS53 RAM19 WORDS row=");
        hex(&uart, index as u32);
        for &v in row {
            emit(&uart, b" ");
            hex(&uart, v);
        }
        emit(&uart, b"\r\n");
    }
    emit(
        &uart,
        b"WS53 RAM19 PASS copied=1 samples=3 pad_restore=1\r\n",
    );
    for _ in 0..1_000_000 {
        if uart.tx_flushed() {
            ws53_ram19_done();
            loop {
                core::hint::spin_loop()
            }
        }
    }
    fail(83)
}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    fail(info.location().map_or(0xffff, |v| 0x10000 | v.line()))
}
