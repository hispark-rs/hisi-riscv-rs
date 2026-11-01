//! Autonomous Flash startup, identical position-independent loop in Flash/ITCM.
//! Reuse only the inactive bootstrap-stack bottom after main has switched stacks.
//! No cache enable/maintenance, PMP, clock or Flash-controller writes.
#![no_std]
#![no_main]
#[cfg(not(feature = "flash-autonomous"))]
compile_error!("execution comparison requires autonomous Flash startup");
#[cfg(feature = "model-only")]
compile_error!("this fixture has no substitute model-only entry");

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
static mut WS53_EXEC_COMPARE: [u32; 256] = [0; 256];
static mut INITIALIZED: u32 = 0x53ec0123;
static mut ZEROED: u32 = 0;
const LOOP: [u32; 3] = [0xfff50513, 0xfe051ee3, 0x00008067];

core::arch::global_asm!(
    r#"
    .section .text.ws53_execution_loop,"ax"
    .option push
    .option norvc
    .balign 32
    .global ws53_execution_loop
    .global ws53_execution_loop_end
    .type ws53_execution_loop,@function
ws53_execution_loop:
1:
    addi a0, a0, -1
    bnez a0, 1b
    ret
ws53_execution_loop_end:
    .size ws53_execution_loop, .-ws53_execution_loop
    .option pop
"#
);
unsafe extern "C" {
    fn ws53_execution_loop(iterations: u32) -> u32;
    fn ws53_execution_loop_end();
    static __bootstrap_stack_bottom__: u32;
    static __bootstrap_stack_top__: u32;
    static __stack_start__: u32;
    static __stack_top__: u32;
    fn trap_vector();
}
fn put(index: usize, value: u32) {
    unsafe {
        addr_of_mut!(WS53_EXEC_COMPARE)
            .cast::<u32>()
            .add(index)
            .write_volatile(value)
    }
}
fn snapshot() -> [u32; 256] {
    core::array::from_fn(|i| unsafe {
        addr_of!(WS53_EXEC_COMPARE)
            .cast::<u32>()
            .add(i)
            .read_volatile()
    })
}
fn fail(code: u32) -> ! {
    put(2, code);
    put(1, 0xee);
    loop {
        core::hint::spin_loop();
    }
}
fn cache_state() -> [u32; 3] {
    let (i, d, p): (u32, u32, u32);
    unsafe {
        core::arch::asm!("csrr {i}, 0x7c0", "csrr {d}, 0x7c1", "csrr {p}, 0x7c6",
        i=out(reg)i, d=out(reg)d, p=out(reg)p, options(nostack));
    }
    [i, d, p]
}
fn cycles() -> u64 {
    loop {
        let (h, l, h2): (u32, u32, u32);
        unsafe {
            core::arch::asm!("csrr {h}, 0xb80", "csrr {l}, 0xb00", "csrr {h2}, 0xb80",
            h=out(reg)h, l=out(reg)l, h2=out(reg)h2, options(nostack));
        }
        if h == h2 {
            return (u64::from(h) << 32) | u64::from(l);
        }
    }
}
fn emit(uart: &Uart<'_, Uart2<'_>>, bytes: &[u8]) {
    for &byte in bytes {
        let mut budget = 1_000_000;
        while uart.try_write_byte(byte).is_err() {
            if budget == 0 {
                fail(80);
            }
            budget -= 1;
        }
    }
}
fn hex(uart: &Uart<'_, Uart2<'_>>, value: u32) {
    for nibble in (0..8).rev() {
        emit(
            uart,
            &[b"0123456789abcdef"[((value >> (nibble * 4)) & 15) as usize]],
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
        fail(81);
    }
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    put(0, 0x53c20001);
    put(3, 0x464c5332);
    put(4, 12);
    put(5, 32_000_000);
    put(9, unsafe { addr_of!(INITIALIZED).read_volatile() });
    put(10, unsafe { addr_of!(ZEROED).read_volatile() });
    if snapshot()[9..11] != [0x53ec0123, 0] {
        fail(1);
    }
    if riscv::register::mstatus::read().mie() {
        fail(2);
    }
    let sp: u32;
    unsafe {
        core::arch::asm!("mv {sp}, sp", sp=out(reg)sp, options(nostack));
    }
    let stack_start = addr_of!(__stack_start__) as u32;
    let stack_top = addr_of!(__stack_top__) as u32;
    put(11, sp);
    put(45, stack_start);
    put(46, stack_top);
    if !(stack_start..stack_top).contains(&sp) || sp & 15 != 0 {
        fail(3);
    }
    if riscv::register::mtvec::read().bits() & !3 != trap_vector as *const () as usize {
        fail(4);
    }
    // Early/NMI bootstrap vectors are no longer installed. The main, IRQ,
    // exception and NMI stacks all reside in DTCM in this fixed layout.
    let ram = addr_of!(__bootstrap_stack_bottom__) as *mut u32;
    let ram_end = addr_of!(__bootstrap_stack_top__) as u32;
    let source = ws53_execution_loop as *const () as *const u32;
    let end = ws53_execution_loop_end as *const () as usize;
    if ram as u32 != 0x2f000
        || ram_end != 0x30000
        || end - source as usize != 12
        || source as usize & 31 != 0
    {
        fail(5);
    }
    put(18, source as u32);
    put(19, ram as u32);
    put(20, 12);
    put(44, ram_end);
    let before = cache_state();
    put(22, riscv::register::mstatus::read().bits() as u32);
    let memory = unsafe { &*ws53_pac::SystemMemory::PTR };
    let mdma = unsafe { &*ws53_pac::Mdma::PTR };
    put(23, memory.ccore_reset_status().read().bits());
    put(24, memory.bram_share().read().bits());
    put(27, mdma.enabled_channels().read().bits());
    put(48, memory.dtcm_share().read().bits());
    put(49, memory.platform_enable().read().bits());
    if snapshot()[23] != 0
        || snapshot()[24] != 0x3f8
        || snapshot()[27] != 0
        || snapshot()[48] != 0x3c
        || snapshot()[49] != 0
    {
        fail(17);
    }
    for (i, value) in before.into_iter().enumerate() {
        put(12 + i, value);
    }
    if before[0] != 6 || before[1] != 0 {
        fail(6);
    }
    // Reserved application ITCM is not the current stack. No active code or
    // shared buffer is overwritten; restore all twelve bytes before parking.
    let old: [u32; 3] = core::array::from_fn(|i| unsafe { ram.add(i).read_volatile() });
    for i in 0..3 {
        let word = unsafe { source.add(i).read_volatile() };
        if word != LOOP[i] {
            fail(7);
        }
        put(32 + i, word);
        put(38 + i, old[i]);
        unsafe {
            ram.add(i).write_volatile(word);
        }
    }
    unsafe {
        core::arch::asm!("fence rw, rw", "fence.i", options(nostack));
    }
    for (i, &expected) in LOOP.iter().enumerate() {
        let copied = unsafe { ram.add(i).read_volatile() };
        put(35 + i, copied);
        if copied != expected {
            fail(8);
        }
    }
    put(7, 1);
    put(6, 12);
    let ram_loop: unsafe extern "C" fn(u32) -> u32 = unsafe { core::mem::transmute(ram) };
    let uart = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    let mut timer = TimerDriver::new(unsafe { p.TIMER.assume_application_ownership() });
    let channel = TimerChannel::Channel1;
    if interrupt::is_enabled(channel.interrupt()) || interrupt::is_pending(channel.interrupt()) {
        fail(9);
    }
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
    let wdt_control = unsafe { &*ws53_pac::Wdt::PTR }.wdt_cr().read().bits();
    put(26, wdt_control);
    if wdt_control & 1 != 0 {
        fail(18);
    }
    timer.disable(channel);
    timer.clear_interrupt(channel);
    interrupt::clear_pending(channel.interrupt());
    put(1, 1);
    capture_delay::wait(&mut timer);
    put(1, 2);
    let mut pad = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    let original = pad.configuration().unwrap();
    let regs = unsafe { &*ws53_pac::Gpio0::PTR };
    if original.mode().mode().code() != 0
        || interrupt::is_enabled(GpioBank::Bank0.interrupt())
        || regs.gpio_int_en().read().bits() != 0
        || regs.gpio_sw_ctl().read().bits() != 0
        || regs.gpio_sw_oen().read().bits() & (1 << 17) == 0
    {
        fail(10);
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
    let pin = pad.into_gpio().unwrap_or_else(|_| fail(11));
    let mut bank = GpioDriver::new_gpio0(unsafe { p.GPIO0.assume_application_ownership() });
    let mut output = bank.init_output(pin, false).unwrap_or_else(|_| fail(12));
    timer.configure(
        channel,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(channel);
    emit(
        &uart,
        b"WS53 EXEC COMPARE BEGIN samples=12 bytes=12 pad=17\r\n",
    );
    put(1, 3);
    let mut sample = 0;
    for pass in 0..2 {
        for iterations in [4096, 16384, 65536] {
            for order in 0..2 {
                let kind = order ^ pass;
                let (call, address): (unsafe extern "C" fn(u32) -> u32, u32) = if kind == 0 {
                    (ws53_execution_loop, source as u32)
                } else {
                    (ram_loop, ram as u32)
                };
                let cycle_before = cycles();
                let timer_before = timer.current_value(channel).unwrap();
                output.set_high();
                riscv::asm::fence();
                let remaining = unsafe { call(iterations) };
                riscv::asm::fence();
                output.set_low();
                let timer_after = timer.current_value(channel).unwrap();
                let cycle_after = cycles();
                let values = [
                    pass,
                    kind,
                    iterations,
                    remaining,
                    timer_before,
                    timer_after,
                    cycle_before as u32,
                    (cycle_before >> 32) as u32,
                    cycle_after as u32,
                    (cycle_after >> 32) as u32,
                    address,
                    sample,
                ];
                for (offset, value) in values.into_iter().enumerate() {
                    put(64 + sample as usize * 12 + offset, value);
                }
                if remaining != 0 || cycle_after <= cycle_before {
                    fail(13);
                }
                Delay(&mut timer).delay_ms(1);
                sample += 1;
            }
        }
    }
    put(1, 4);
    let _pin = output.free();
    let mut restored = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    restored.configure(original).unwrap();
    if restored.configuration().unwrap() != original {
        fail(14);
    }
    put(28, 1);
    timer.disable(channel);
    timer.clear_interrupt(channel);
    interrupt::clear_pending(channel.interrupt());
    put(29, 1);
    for (i, &value) in old.iter().enumerate() {
        unsafe {
            ram.add(i).write_volatile(value);
        }
    }
    unsafe {
        core::arch::asm!("fence rw, rw", "fence.i", options(nostack));
    }
    for (i, &expected) in old.iter().enumerate() {
        let word = unsafe { ram.add(i).read_volatile() };
        put(41 + i, word);
        if word != expected {
            fail(15);
        }
    }
    put(8, 1);
    let after = cache_state();
    put(25, riscv::register::mstatus::read().bits() as u32);
    for (i, value) in after.into_iter().enumerate() {
        put(15 + i, value);
    }
    if before != after || riscv::register::mstatus::read().mie() {
        fail(16);
    }
    put(1, 5);
    for (row, chunk) in snapshot().chunks(16).enumerate() {
        emit(&uart, b"WS53 EXEC WORDS row=");
        hex(&uart, row as u32);
        for word in chunk {
            emit(&uart, b" ");
            hex(&uart, *word);
        }
        emit(&uart, b"\r\n");
    }
    emit(
        &uart,
        b"WS53 EXEC COMPARE PASS samples=12 ram_restore=1 pad_restore=1\r\n",
    );
    for _ in 0..1_000_000 {
        if uart.tx_flushed() {
            loop {
                core::hint::spin_loop();
            }
        }
    }
    fail(82);
}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    fail(info.location().map_or(0xffff, |l| 0x10000 | l.line()))
}
