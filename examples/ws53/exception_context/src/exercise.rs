//! Shared test logic; transport/boot/ownership belongs to the caller.
use core::ptr::{addr_of, addr_of_mut};
#[cfg(feature = "irq-exception")]
use hisi_hal::{
    interrupt,
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
};
#[cfg(not(any(
    feature = "legacy-hooks",
    feature = "named-stop",
    feature = "default-stop",
    feature = "default-halt"
)))]
use hisi_riscv_rt::ExceptionContext;

const _: () = assert!(
    (cfg!(feature = "legacy-hooks") as u8
        + cfg!(feature = "named-stop") as u8
        + cfg!(feature = "default-stop") as u8
        + cfg!(feature = "default-halt") as u8)
        <= 1,
    "select one exception hook profile"
);
const _: () = assert!(
    !cfg!(feature = "irq-exception")
        || !(cfg!(feature = "legacy-hooks")
            || cfg!(feature = "named-stop")
            || cfg!(feature = "default-stop")
            || cfg!(feature = "default-halt")),
    "IRQ exception test requires typed hook"
);
core::arch::global_asm!(include_str!("probe.S"));
unsafe extern "C" {
    fn ws53_exception_probe(mode: u32) -> u32;
    fn ws53_nested_fault();
    static WS53_EXC_FAULT16: u8;
    static WS53_EXC_AFTER16: u8;
    static WS53_EXC_FAULT32: u8;
    static WS53_EXC_AFTER32: u8;
    static WS53_EXC_ECALL: u8;
    static WS53_EXC_AFTER_ECALL: u8;
    static WS53_EXC_NESTED: u8;
    static WS53_EXC_AFTER_NESTED: u8;
    #[cfg(not(feature = "default-halt"))]
    static __exc_stack_start__: u8;
    #[cfg(not(feature = "default-halt"))]
    static __exc_stack_top__: u8;
}
#[used]
#[unsafe(no_mangle)]
static mut WS53_EXCEPTION_REPORT: [u32; 128] = [0; 128];
#[used]
#[unsafe(no_mangle)]
static mut WS53_EXCEPTION_EXPECTED_SP: u32 = 0;
#[used]
#[unsafe(no_mangle)]
static WS53_EXCEPTION_ENVIRONMENT: u32 = if cfg!(feature = "hil-controlled") {
    0x48494c31
} else if cfg!(feature = "flash-autonomous") {
    0x464c5331
} else {
    0x51454d31
};
#[used]
#[unsafe(no_mangle)]
static mut WS53_EXCEPTION_HIL_GATE: u32 = 0;

fn put(index: usize, value: u32) {
    unsafe {
        addr_of_mut!(WS53_EXCEPTION_REPORT)
            .cast::<u32>()
            .add(index)
            .write_volatile(value)
    }
}
#[cfg(not(feature = "default-halt"))]
fn get(index: usize) -> u32 {
    unsafe {
        addr_of!(WS53_EXCEPTION_REPORT)
            .cast::<u32>()
            .add(index)
            .read_volatile()
    }
}
pub fn park() -> ! {
    loop {
        core::hint::spin_loop()
    }
}
#[cfg(not(feature = "default-halt"))]
fn check(ok: bool) {
    if !ok {
        put(4, get(4) + 1);
    }
}
#[cfg(not(feature = "default-halt"))]
fn expected_scratch() -> u32 {
    if get(15) == 1 { get(16) } else { 0x13579bdf }
}
#[cfg(not(feature = "default-halt"))]
fn handler_metadata() {
    let sp: u32;
    let status: u32;
    let scratch: u32;
    unsafe {
        core::arch::asm!("mv {s}, sp","csrr {m}, mstatus","csrr {r}, mscratch",
        s=out(reg)sp,m=out(reg)status,r=out(reg)scratch,options(nostack));
    }
    check(
        sp.is_multiple_of(16)
            && sp >= addr_of!(__exc_stack_start__) as u32
            // A leaf hook need not allocate a frame; SP==top is valid.
            && sp <= addr_of!(__exc_stack_top__) as u32,
    );
    check(status & 8 == 0 && scratch == expected_scratch());
    put(9, sp);
    put(12, scratch);
    put(5, get(5) + 1);
}

#[cfg(not(any(
    feature = "legacy-hooks",
    feature = "named-stop",
    feature = "default-stop",
    feature = "default-halt"
)))]
#[unsafe(no_mangle)]
unsafe extern "C" fn WS53_EXCEPTION(ctx: &mut ExceptionContext) {
    handler_metadata();
    let depth = get(6) + 1;
    put(6, depth);
    put(7, get(7).max(depth));
    put(8, ctx as *mut ExceptionContext as u32);
    put(10, ctx.cause());
    put(11, ctx.program_counter());
    let raw_cause: u32;
    let raw_pc: u32;
    let raw_value: u32;
    unsafe {
        core::arch::asm!("csrr {c}, mcause","csrr {p}, mepc","csrr {v}, mtval",
                        c=out(reg)raw_cause,p=out(reg)raw_pc,v=out(reg)raw_value,options(nostack));
    }
    check(
        ctx.cause() == raw_cause
            && ctx.program_counter() == raw_pc
            && ctx.trap_value() == raw_value,
    );
    check(
        ctx.status() & 8 == 0
            && ctx.status() & 0x1800 == 0x1800
            && ctx.previous_scratch() == expected_scratch(),
    );
    if ctx.program_counter() == addr_of!(WS53_EXC_NESTED) as u32 {
        check(
            ctx.cause() == 2 && depth == if get(15) == 1 { 1 } else { 2 } && ctx.trap_value() == 0,
        );
        if get(15) == 1 {
            put(17, ctx.previous_scratch());
        }
        unsafe {
            ctx.set_program_counter(addr_of!(WS53_EXC_AFTER_NESTED) as u32);
        }
    } else {
        let mode = get(2);
        let kind = mode & 3;
        let sites = [
            addr_of!(WS53_EXC_FAULT16) as u32,
            addr_of!(WS53_EXC_FAULT32) as u32,
            addr_of!(WS53_EXC_ECALL) as u32,
        ];
        let returns = [
            addr_of!(WS53_EXC_AFTER16) as u32,
            addr_of!(WS53_EXC_AFTER32) as u32,
            addr_of!(WS53_EXC_AFTER_ECALL) as u32,
        ];
        check(
            ctx.program_counter() == sites[kind as usize]
                && ctx.cause() == if kind == 2 { 11 } else { 2 },
        );
        check(
            ctx.status() & 0x6000 == ((mode >> 4) & 3) << 13
                && ((ctx.status() >> 7) & 1) == ((mode >> 8) & 1),
        );
        check(ctx.floating_status() == 0x65);
        // RISC-V permits an implementation to report zero for an illegal
        // instruction. Capture the actual value; do not impose QEMU's choice
        // on silicon or accept unrelated/non-instruction values.
        let expected_value = if kind == 1 { 0xc0001073 } else { 0 };
        check(
            ctx.trap_value() == expected_value
                || ((cfg!(feature = "hil-controlled") || cfg!(feature = "flash-autonomous"))
                    && ctx.trap_value() == 0),
        );
        let index = get(3) as usize;
        put(32 + index, ctx.trap_value());
        put(64 + index, ctx.status());
        put(96 + index, get(9));
        for (index, value) in ctx.integer_registers().iter().enumerate() {
            let expected = match index {
                0 => 0,
                2 => unsafe { addr_of!(WS53_EXCEPTION_EXPECTED_SP).read_volatile() },
                _ => 0x53000000 + index as u32,
            };
            check(*value == expected);
        }
        for (index, value) in ctx.floating_registers().iter().enumerate() {
            check(*value == 0x3f000000 + index as u32);
        }
        if mode & 0x800 != 0 {
            let canary = [0x5353a5a5u32; 16];
            let original_pc = ctx.program_counter();
            unsafe {
                ws53_nested_fault();
            }
            check(ctx.program_counter() == original_pc && ctx.cause() == 11);
            for i in 0..16 {
                check(unsafe { canary.as_ptr().add(i).read_volatile() } == 0x5353a5a5);
            }
            put(13, get(13) + 1);
        }
        unsafe {
            ctx.set_program_counter(returns[kind as usize]);
        }
    }
    put(6, get(6) - 1);
}

#[cfg(feature = "legacy-hooks")]
#[unsafe(no_mangle)]
extern "C" fn do_ecall_m() {
    handler_metadata();
}
#[cfg(feature = "named-stop")]
#[unsafe(no_mangle)]
extern "C" fn do_insn_illegal() {
    handler_metadata();
    put(1, 2);
    park()
}
#[cfg(feature = "default-stop")]
#[unsafe(no_mangle)]
extern "C" fn default_exc_handler() {
    handler_metadata();
    put(1, 2);
    park()
}

#[cfg(feature = "irq-exception")]
#[unsafe(no_mangle)]
extern "C" fn TIMER_0() {
    unsafe {
        TimerDriver::acknowledge_interrupt(TimerChannel::Channel0);
    }
    let before: u32;
    let sp: u32;
    unsafe {
        core::arch::asm!("csrr {r}, mscratch","mv {s}, sp",r=out(reg)before,s=out(reg)sp,options(nostack));
    }
    put(15, 1);
    put(16, before);
    put(29, sp);
    unsafe {
        ws53_nested_fault();
    }
    let after: u32;
    let restored_sp: u32;
    unsafe {
        core::arch::asm!("csrr {r}, mscratch","mv {s}, sp",r=out(reg)after,s=out(reg)restored_sp,options(nostack));
    }
    put(18, after);
    put(30, restored_sp);
    check(before == after && sp == restored_sp);
    put(15, 0);
    put(14, get(14) + 1);
}

#[cfg(feature = "irq-exception")]
pub fn irq_probe(timer: &mut TimerDriver<'_>) {
    let channel = TimerChannel::Channel0;
    timer.configure(
        channel,
        TimerMode::OneShot,
        TimerTicks::from_micros(1000).unwrap(),
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
    while get(14) == 0 {
        budget -= 1;
        if budget == 0 {
            put(4, 998);
            put(1, 0xee);
            park();
        }
        core::hint::spin_loop();
    }
    riscv::interrupt::disable();
    timer.disable(channel);
    timer.clear_interrupt(channel);
    unsafe {
        interrupt::disable(channel.interrupt());
    }
    check(get(14) == 1);
}

pub fn run_probes() {
    put(0, 0x530e0001);
    let environment = unsafe { addr_of!(WS53_EXCEPTION_ENVIRONMENT).read_volatile() };
    put(31, environment);
    // BSS initialization precedes this rendezvous. The host must observe the
    // ready phase and then write the nonce; preloading a BSS gate is invalid.
    if cfg!(feature = "hil-controlled") {
        put(1, 0x10);
        while unsafe { addr_of!(WS53_EXCEPTION_HIL_GATE).read_volatile() } != 0x53e1c0de {
            core::hint::spin_loop();
        }
    }
    put(1, 1);
    // Retain every test symbol in all profiles, including the default halt.
    put(20, addr_of!(WS53_EXC_FAULT16) as u32);
    put(21, addr_of!(WS53_EXC_AFTER16) as u32);
    put(22, addr_of!(WS53_EXC_FAULT32) as u32);
    put(23, addr_of!(WS53_EXC_AFTER32) as u32);
    put(24, addr_of!(WS53_EXC_ECALL) as u32);
    put(25, addr_of!(WS53_EXC_AFTER_ECALL) as u32);
    put(26, addr_of!(WS53_EXC_NESTED) as u32);
    put(27, addr_of!(WS53_EXC_AFTER_NESTED) as u32);
    put(28, cfg!(feature = "irq-exception") as u32);
    let _nested = ws53_nested_fault as *const ();
    #[cfg(any(
        feature = "named-stop",
        feature = "default-stop",
        feature = "default-halt"
    ))]
    {
        let _result = unsafe { ws53_exception_probe(0) };
        put(4, 1000);
        put(1, 0xee);
        park()
    }
    #[cfg(not(any(
        feature = "named-stop",
        feature = "default-stop",
        feature = "default-halt"
    )))]
    {
        for kind in 0..3 {
            if cfg!(feature = "legacy-hooks") && kind != 2 {
                continue;
            }
            for fs in 0..4 {
                for mie in 0..2 {
                    let mode = kind | (fs << 4) | (mie << 8);
                    put(2, mode);
                    let errors = unsafe { ws53_exception_probe(mode) };
                    put(4, get(4) + errors);
                    put(3, get(3) + 1);
                }
            }
        }
        #[cfg(not(feature = "legacy-hooks"))]
        {
            put(2, 0x832);
            let errors = unsafe { ws53_exception_probe(0x832) };
            put(4, get(4) + errors);
            put(3, get(3) + 1);
        }
    }
}
pub fn fail() -> ! {
    put(4, 999);
    put(1, 0xee);
    park()
}

pub fn finish() {
    put(1, 2);
}
#[cfg(feature = "flash-autonomous")]
pub fn snapshot() -> [u32; 128] {
    core::array::from_fn(get)
}
