//! BS21 bare-metal TIMER channel-0 LOCI interrupt smoke test (no embassy).
//!
//! Discriminator for the embassy bring-up: programs TIMER channel 0 directly,
//! enables IRQ 53 (TIMER_0, a LOCI custom local interrupt), and busy-waits. The
//! trap handler prints on the alarm and clears it. If "IRQ FIRED" prints, the
//! bare LOCI+timer delivery path works on `-M bs21`.

#![no_std]
#![no_main]

use hisi_riscv_hal::interrupt::{self, Interrupt};
use hisi_riscv_rt::entry;

const UART0_DATA: *mut u32 = 0x5208_1004 as *mut u32;
const UART0_FIFO: *const u32 = 0x5208_1044 as *const u32;

// TIMER channel 0 (base 0x5200_2000, channel 0 at +0x100).
const T0_LOAD: *mut u32 = 0x5200_2100 as *mut u32;
const T0_CTRL: *mut u32 = 0x5200_2110 as *mut u32;
const T0_EOI: *const u32 = 0x5200_2114 as *const u32;

fn putc(b: u8) {
    unsafe {
        while core::ptr::read_volatile(UART0_FIFO) & 1 != 0 {}
        core::ptr::write_volatile(UART0_DATA, b as u32);
    }
}
fn puts(s: &[u8]) {
    for &b in s {
        putc(b);
    }
}

core::arch::global_asm!(
    ".section .text.atrap, \"ax\"",
    ".balign 4",
    ".global atrap",
    "atrap:",
    "    addi sp, sp, -16",
    "    sw ra,0(sp)\n sw t0,4(sp)\n sw t1,8(sp)\n sw a0,12(sp)",
    "    call atrap_handle",
    "    lw ra,0(sp)\n lw t0,4(sp)\n lw t1,8(sp)\n lw a0,12(sp)",
    "    addi sp, sp, 16",
    "    mret",
);

unsafe extern "C" {
    fn atrap();
}

#[unsafe(no_mangle)]
extern "C" fn atrap_handle() {
    let mcause: u32;
    unsafe { core::arch::asm!("csrr {0}, mcause", out(reg) mcause) };
    puts(b"\r\n[TRAP] mcause=");
    // print mcause low byte in hex
    let lo = (mcause & 0xFF) as u8;
    let hex = b"0123456789abcdef";
    putc(hex[(lo >> 4) as usize]);
    putc(hex[(lo & 0xF) as usize]);
    if (mcause & 0x8000_0000) != 0 && (mcause & 0xFFF) == 53 {
        puts(b" IRQ FIRED (TIMER_0=53)\r\n");
        unsafe {
            let _ = core::ptr::read_volatile(T0_EOI); // clear timer
            core::ptr::write_volatile(T0_CTRL, 0); // stop
        }
        interrupt::clear_pending(Interrupt::TIMER_0);
    } else {
        puts(b" (other)\r\n");
    }
}

#[entry]
fn main() -> ! {
    puts(b"\r\nBS21 timer_irq: programming TIMER0 ch0 + IRQ 53...\r\n");
    unsafe {
        core::arch::asm!("csrw mtvec, {0}", in(reg) atrap as *const () as usize);
        interrupt::init();
        // Program a one-shot ~ (load / 24MHz) seconds. 480000 / 24e6 = 20ms.
        core::ptr::write_volatile(T0_CTRL, 0); // stop first
        let _ = core::ptr::read_volatile(T0_EOI);
        core::ptr::write_volatile(T0_LOAD, 480_000);
        core::ptr::write_volatile(T0_CTRL, 1); // EN, unmasked
        interrupt::enable(Interrupt::TIMER_0);
        interrupt::enable_global();
    }
    puts(b"BS21 timer_irq: armed, spinning (expect IRQ within ~20ms)...\r\n");
    let mut spins: u32 = 0;
    loop {
        spins = spins.wrapping_add(1);
        if spins.is_multiple_of(2_000_000) {
            putc(b'.');
        }
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
