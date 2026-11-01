//! QEMU-only software trap injection. NEVER use as a physical NMI test.
#![no_std]
#![no_main]

use hisi_hal::timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks};
use hisi_hal::uart::Uart;
use hisi_hal::{Peripherals, interrupt};
#[cfg(not(feature = "default-hook-test"))]
use hisi_riscv_rt::NmiContext;
use hisi_riscv_rt::entry;

core::arch::global_asm!(include_str!("injection.S"));
unsafe extern "C" {
    fn ws53_test_inject_nmi(mode: u32) -> u32;
    #[cfg(not(feature = "default-hook-test"))]
    static __nmi_stack_start__: u8;
    #[cfg(not(feature = "default-hook-test"))]
    static __nmi_stack_top__: u8;
}

// Test-owned RAM, accessed only in the stated main/IRQ/NMI ordering. Ordinary
// critical-section locks are deliberately not used by the NMI hook.
static mut DEPTH: u32 = 0;
static mut CALLS: u32 = 0;
static mut ERRORS: u32 = 0;
static mut TICKS: u32 = 0;

#[cfg(not(feature = "default-hook-test"))]
#[unsafe(no_mangle)]
unsafe extern "C" fn WS53_NMI(ctx: &NmiContext) {
    unsafe {
        let depth = core::ptr::addr_of!(DEPTH).read_volatile();
        core::ptr::addr_of_mut!(DEPTH).write_volatile(depth + 1);
        let calls = core::ptr::addr_of!(CALLS).read_volatile();
        core::ptr::addr_of_mut!(CALLS).write_volatile(calls + 1);
        let mut errors = 0;
        for (index, value) in ctx.integer_registers().iter().enumerate() {
            if index != 2
                && *value
                    != if index == 0 {
                        0
                    } else {
                        0x5300_0000 | index as u32
                    }
            {
                errors += 1;
            }
        }
        for (index, value) in ctx.floating_registers().iter().enumerate() {
            if *value != 0x3f00_0000 | index as u32 {
                errors += 1;
            }
        }
        let scratch: u32;
        core::arch::asm!("csrr {}, mscratch", out(reg) scratch);
        let handler_sp: usize;
        core::arch::asm!("mv {}, sp", out(reg) handler_sp);
        let mut guard = [0x5353_a5a5u32; 16];
        let here = guard.as_ptr() as usize;
        if here < core::ptr::addr_of!(__nmi_stack_start__) as usize
            || here + core::mem::size_of_val(&guard)
                > core::ptr::addr_of!(__nmi_stack_top__) as usize
            || !handler_sp.is_multiple_of(16)
            || ctx.cause() != 0x8000_000c
            || ctx.trap_value() != 0x53ab_cde0
            || ctx.status() & 8 != 0
            || ctx.floating_status() != 0x65
            || ctx.previous_scratch() != scratch
        {
            errors += 1;
        }
        if depth == 0 {
            // This is explicit SOFTWARE reentry from a hook, not hardware
            // redelivery. The outer hook's locals/return address must survive.
            errors += ws53_test_inject_nmi(2);
        }
        for value in &mut guard {
            if core::ptr::from_ref(value).read_volatile() != 0x5353_a5a5 {
                errors += 1;
            }
        }
        core::arch::asm!(
            "li t0, 0x41234567", "fmv.w.x f0, t0", "fmv.w.x f7, t0",
            "fmv.w.x f10, t0", "fmv.w.x f17, t0", "fmv.w.x f28, t0",
            "fmv.w.x f31, t0", "csrwi fcsr, 0",
            out("t0") _, out("f0") _, out("f7") _, out("f10") _,
            out("f17") _, out("f28") _, out("f31") _);
        let prior = core::ptr::addr_of!(ERRORS).read_volatile();
        core::ptr::addr_of_mut!(ERRORS).write_volatile(prior + errors);
        core::ptr::addr_of_mut!(DEPTH).write_volatile(depth);
    }
}

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let uart = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    let emit = |bytes: &[u8]| {
        for &b in bytes {
            let mut budget = 1_000_000;
            while uart.try_write_byte(b).is_err() {
                budget -= 1;
                if budget == 0 {
                    fail();
                }
            }
        }
    };
    emit(b"WS53 NMI SOFTWARE injection begin\r\n");
    // No peripheral IRQ is enabled yet. Exercise MPIE->MIE restoration to 1
    // here and to 0 in the timer ISR/nested hook cases below.
    unsafe {
        riscv::interrupt::enable();
    }
    for mode in 0..3 {
        if unsafe { ws53_test_inject_nmi(mode) } != 0 {
            emit(b"WS53 FAIL NMI main\r\n");
            fail();
        }
    }
    riscv::interrupt::disable();
    if unsafe { core::ptr::addr_of!(ERRORS).read_volatile() } != 0 {
        emit(b"WS53 FAIL NMI hook\r\n");
        fail();
    }
    emit(b"WS53 PASS NMI main modes=3 nested=3\r\n");
    let mut timer = TimerDriver::new(unsafe { p.TIMER.assume_application_ownership() });
    let channel = TimerChannel::Channel0;
    timer.configure(
        channel,
        TimerMode::Periodic,
        TimerTicks::from_micros(10_000).unwrap(),
    );
    interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
    unsafe {
        interrupt::enable(channel.interrupt());
    }
    timer.enable(channel);
    unsafe {
        riscv::interrupt::enable();
    }
    let mut budget = 100_000_000;
    while unsafe { core::ptr::addr_of!(TICKS).read_volatile() } < 3 {
        budget -= 1;
        if budget == 0 {
            emit(b"WS53 FAIL NMI timer timeout\r\n");
            fail();
        }
    }
    riscv::interrupt::disable();
    timer.disable(channel);
    timer.clear_interrupt(channel);
    unsafe {
        interrupt::disable(channel.interrupt());
    }
    if unsafe { core::ptr::addr_of!(ERRORS).read_volatile() } != 0
        || unsafe { core::ptr::addr_of!(CALLS).read_volatile() } != 12
        || unsafe { core::ptr::addr_of!(DEPTH).read_volatile() } != 0
    {
        emit(b"WS53 FAIL NMI IRQ state\r\n");
        fail();
    }
    emit(b"WS53 PASS NMI software contexts=12 irq=53 fs=off,clean,dirty\r\n");
    loop {
        riscv::asm::wfi();
    }
}

#[unsafe(no_mangle)]
extern "C" fn TIMER_0() {
    unsafe {
        TimerDriver::acknowledge_interrupt(TimerChannel::Channel0);
        let ticks = core::ptr::addr_of!(TICKS).read_volatile();
        if ticks < 3 {
            let errors = ws53_test_inject_nmi(ticks);
            let prior = core::ptr::addr_of!(ERRORS).read_volatile();
            core::ptr::addr_of_mut!(ERRORS).write_volatile(prior + errors);
        }
        core::ptr::addr_of_mut!(TICKS).write_volatile(ticks + 1);
    }
}

fn fail() -> ! {
    loop {
        core::hint::spin_loop();
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    fail()
}
