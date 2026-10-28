//! QEMU instruction-experiment ONLY. Not a physical RAM/HIL entry point.
//! All image code/literals/traps are in RAM. The sole external executable is
//! the host's controlled Flash probe, called outside cache transitions.
#![no_std]
#![no_main]

#[cfg(not(feature = "qemu-only"))]
compile_error!("cache_contract requires qemu-only; no hardware qualification");

use core::ptr::{addr_of, addr_of_mut};
use hisi_riscv_rt::{
    entry,
    ws53_cache::{
        InstructionConfig, InstructionError, InstructionObservation, InstructionSize,
        configure_instruction_cache,
    },
};

#[used]
#[unsafe(no_mangle)]
static mut WS53_CACHE_CONTRACT: [u32; 64] = [0; 64];
static mut DATA_SENTINEL: u32 = 0x53ca_0123;
static mut BSS_SENTINEL: [u32; 4] = [0; 4];
const MAGIC: u32 = 0x530c_0001;
const NONCE: u32 = 0x53ca_0061;
const PROBE: usize = 0x004f_f000;

// Private, stackless failure capture. It does not override the runtime's weak
// assembly definitions in an LTO unit or test its general exception dispatch.
core::arch::global_asm!(
    ".section .text.ws53_cache_contract_trap,\"ax\"",
    ".balign 4",
    ".global WS53_CACHE_TRAP",
    ".type WS53_CACHE_TRAP,@function",
    "WS53_CACHE_TRAP:",
    "la t0, WS53_CACHE_CONTRACT",
    "csrr t1, mcause",
    "sw t1, 92(t0)",
    "csrr t1, mepc",
    "sw t1, 96(t0)",
    "csrr t1, mtval",
    "sw t1, 100(t0)",
    "li t1, 9",
    "sw t1, 16(t0)",
    "li t1, 238",
    "sw t1, 4(t0)",
    "1: j 1b",
    ".size WS53_CACHE_TRAP, .-WS53_CACHE_TRAP",
);
unsafe extern "C" {
    fn WS53_CACHE_TRAP();
}

fn put(index: usize, value: u32) {
    // One application hart; no Rust references into this host/guest mailbox.
    // The host only changes command/nonce while this CPU is stopped.
    unsafe {
        addr_of_mut!(WS53_CACHE_CONTRACT)
            .cast::<u32>()
            .add(index)
            .write_volatile(value)
    }
}
fn get(index: usize) -> u32 {
    unsafe {
        addr_of!(WS53_CACHE_CONTRACT)
            .cast::<u32>()
            .add(index)
            .read_volatile()
    }
}
fn stop() -> ! {
    loop {
        core::hint::spin_loop();
    }
}
fn fail(code: u32) -> ! {
    put(4, code);
    put(1, 0xee);
    stop()
}
fn wait(phase: u32) {
    put(1, phase);
    while get(2) != phase {
        core::hint::spin_loop();
    }
    if get(3) != NONCE {
        fail(1);
    }
}
fn observation(index: usize, result: InstructionObservation, expected: [u32; 4]) {
    let actual = [
        result.before_control,
        result.before_prefetch,
        result.after_control,
        result.after_prefetch,
    ];
    for (offset, value) in actual.iter().enumerate() {
        put(index + offset, *value);
    }
    if actual != expected {
        fail(2);
    }
}

#[inline(never)]
fn configure(config: InstructionConfig) -> InstructionObservation {
    // ONLY the explicit QEMU instruction experiment establishes this fixture's
    // synthetic ownership/mappings and instantaneous maintenance contract.
    // The complete Rust call closure and exception path are independent RAM.
    // No DMA/control-core/instruction writer runs during this call; host ROM
    // changes occur only at stopped-CPU phase boundaries outside this closure.
    match unsafe { configure_instruction_cache(config) } {
        Ok(result) => result,
        Err(_) => fail(3),
    }
}
#[inline(never)]
fn reject_enabled() {
    // Same exclusive owner/RAM prerequisites. No transition is allowed with
    // ICEN already set; this call must refuse without invalidating old bytes.
    if unsafe { configure_instruction_cache(InstructionConfig::sdk_application()) }
        != Err(InstructionError::AlreadyEnabled)
    {
        fail(4);
    }
}
#[inline(never)]
fn probe() -> u32 {
    // Host-installed first-party model code, never a vendor ROM/Flash helper.
    // It is called only AFTER a cache operation returns to the RAM caller.
    let function: unsafe extern "C" fn() -> u32 = unsafe { core::mem::transmute(PROBE) };
    unsafe { function() }
}
#[inline(never)]
fn model_disable(size: InstructionSize) {
    // Model scaffolding, NOT a qualified public cache-disable driver. Retain
    // modeled instruction lines while clearing ICEN, to test that the real
    // Rust configuration routine subsequently invalidates those stale lines.
    // This does not prove hardware disable, instruction quiescence or drain.
    unsafe { core::arch::asm!("csrw 0x7c0, {value}", value=in(reg)size as u32, options(nostack)) }
}

#[entry]
fn main() -> ! {
    let data = unsafe { addr_of!(DATA_SENTINEL).read_volatile() };
    let bss = unsafe { addr_of!(BSS_SENTINEL).cast::<u32>().read_volatile() };
    put(0, MAGIC);
    put(28, data);
    if data != 0x53ca_0123 || bss != 0 {
        fail(5);
    }
    put(5, 1);
    let status: u32;
    let sp: u32;
    unsafe {
        core::arch::asm!("csrr {status}, mstatus", "mv {sp}, sp",
                        status=out(reg)status, sp=out(reg)sp, options(nostack));
    }
    put(6, status);
    put(26, sp);
    if status & 8 != 0 {
        fail(6);
    }
    // Already-initialized RAM, no interrupt/NMI source in this model fixture.
    unsafe {
        core::arch::asm!("csrw mtvec, {vector}",
                        vector=in(reg)WS53_CACHE_TRAP as *const () as usize,
                        options(nostack));
    }
    if InstructionSize::from_kib(64).is_some()
        || InstructionConfig::new(InstructionSize::KiB32, 0).is_some()
        || InstructionConfig::new(InstructionSize::KiB32, 9).is_some()
    {
        fail(7);
    }
    put(7, 1);
    wait(1); // No cache CSR or external Flash probe before explicit handshake.

    observation(
        8,
        configure(InstructionConfig::sdk_application()),
        [6, 0, 7, 7],
    );
    put(12, probe());
    wait(2); // Host changes probe1 -> probe2, retaining modeled instruction bytes.

    put(13, probe());
    reject_enabled();
    put(14, probe());
    put(7, get(7) | 2);
    wait(3);

    model_disable(InstructionSize::KiB32);
    observation(
        15,
        configure(InstructionConfig::sdk_application()),
        [6, 7, 7, 7],
    );
    put(19, probe());
    wait(4); // Host changes probe2 -> probe3.

    model_disable(InstructionSize::KiB32);
    let small = InstructionConfig::new(InstructionSize::KiB8, 1).unwrap();
    observation(30, configure(small), [6, 7, 3, 1]);
    put(34, probe());
    wait(5); // Host changes probe3 -> probe4.

    reject_enabled();
    put(35, probe());
    model_disable(InstructionSize::KiB8);
    observation(
        36,
        configure(InstructionConfig::sdk_application()),
        [2, 1, 7, 7],
    );
    put(40, probe());
    model_disable(InstructionSize::KiB32);
    let control: u32;
    let prefetch: u32;
    let data_control: u32;
    unsafe {
        core::arch::asm!("csrr {c}, 0x7c0", "csrr {p}, 0x7c6", "csrr {d}, 0x7c1",
                        c=out(reg)control, p=out(reg)prefetch, d=out(reg)data_control,
                        options(nostack));
    }
    put(20, control);
    put(21, prefetch);
    put(22, data_control);
    if [
        get(12),
        get(13),
        get(14),
        get(19),
        get(34),
        get(35),
        get(40),
    ] != [1, 1, 1, 2, 3, 3, 4]
        || [control, prefetch, data_control] != [6, 7, 0]
    {
        fail(8);
    }
    put(1, 6);
    stop()
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    fail(10)
}
