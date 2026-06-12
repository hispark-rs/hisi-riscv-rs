//! HiSilicon BS21 **embassy** multitask demo.
//!
//! The BS21 analogue of `examples/ws63/embassy_multitask`: runs `embassy-executor`
//! (platform-riscv32, thread mode) with two async tasks that
//! `embassy_time::Timer::after_millis(..).await` at different rates. Time comes
//! from `hisi_riscv_hal::embassy` — the same chip-neutral embassy-time `Driver`
//! WS63 uses (now() via the TCXO 64-bit counter, alarms via a TIMER channel),
//! built with `--features chip-bs21` so the TCXO rate (32 MHz) and the alarm IRQ
//! come from `soc/bs21.rs`.
//!
//! The only per-chip difference from the WS63 version is the alarm interrupt
//! number: WS63 uses `TIMER_INT0` = 26 (a standard `mie`-bit local interrupt),
//! BS21 uses `TIMER_0` = 53 (a HiSilicon LOCI custom local interrupt). Both are
//! delivered with `mcause = <irq>`, so the trap handler routes the alarm by
//! testing `mcause & 0xFFF == hisi_riscv_hal::embassy::ALARM_IRQ` — no chip
//! literals in this file.

#![no_std]
#![no_main]

use embassy_executor::{Executor, Spawner};
use embassy_time::Timer;
use hisi_riscv_hal::Peripherals;
use hisi_riscv_hal::interrupt;
use hisi_riscv_rt::entry;
use static_cell::StaticCell;

// ── raw UART0 output (BS2X UART_L0 @ 0x5208_1000; avoids sharing a Uart handle
//    across tasks). DATA @ +0x04, FIFO_STATUS @ +0x44, TX-full = bit0 — same
//    16C550-style layout the chip-neutral `uart` driver polls. ──
const UART0_DATA: *mut u32 = 0x5208_1004 as *mut u32;
const UART0_FIFO: *const u32 = 0x5208_1044 as *const u32;

fn putc(b: u8) {
    unsafe {
        while core::ptr::read_volatile(UART0_FIFO) & 1 != 0 {} // TX FIFO full?
        core::ptr::write_volatile(UART0_DATA, b as u32);
    }
}
fn puts(s: &[u8]) {
    for &b in s {
        putc(b);
    }
}
fn putnum(mut n: u32) {
    if n == 0 {
        putc(b'0');
        return;
    }
    let mut buf = [0u8; 10];
    let mut i = buf.len();
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    puts(&buf[i..]);
}

// ── alarm-channel trap (direct mode) ──
core::arch::global_asm!(
    ".section .text.atrap, \"ax\"",
    ".balign 4",
    ".global atrap",
    "atrap:",
    "    addi sp, sp, -64",
    "    sw ra,0(sp)\n sw t0,4(sp)\n sw t1,8(sp)\n sw t2,12(sp)\n sw t3,16(sp)",
    "    sw t4,20(sp)\n sw t5,24(sp)\n sw t6,28(sp)\n sw a0,32(sp)\n sw a1,36(sp)",
    "    sw a2,40(sp)\n sw a3,44(sp)\n sw a4,48(sp)\n sw a5,52(sp)\n sw a6,56(sp)\n sw a7,60(sp)",
    "    call atrap_handle",
    "    lw ra,0(sp)\n lw t0,4(sp)\n lw t1,8(sp)\n lw t2,12(sp)\n lw t3,16(sp)",
    "    lw t4,20(sp)\n lw t5,24(sp)\n lw t6,28(sp)\n lw a0,32(sp)\n lw a1,36(sp)",
    "    lw a2,40(sp)\n lw a3,44(sp)\n lw a4,48(sp)\n lw a5,52(sp)\n lw a6,56(sp)\n lw a7,60(sp)",
    "    addi sp, sp, 64",
    "    mret",
);

unsafe extern "C" {
    fn atrap();
}

#[unsafe(no_mangle)]
extern "C" fn atrap_handle() {
    let mcause: u32;
    unsafe { core::arch::asm!("csrr {0}, mcause", out(reg) mcause) };
    // Interrupt (bit31) and cause == the embassy alarm IRQ (53 on BS21).
    if (mcause & 0x8000_0000) != 0 && (mcause & 0xFFF) == hisi_riscv_hal::embassy::ALARM_IRQ {
        hisi_riscv_hal::embassy::on_alarm_interrupt(); // embassy-time alarm fired
    }
}

#[embassy_executor::task]
async fn fast() {
    let mut n = 0u32;
    loop {
        Timer::after_millis(10).await;
        n += 1;
        puts(b"[fast] tick ");
        putnum(n);
        puts(b"\r\n");
    }
}

#[embassy_executor::task]
async fn slow() {
    let mut n = 0u32;
    loop {
        Timer::after_millis(25).await;
        n += 1;
        puts(b"[slow] tick ");
        putnum(n);
        puts(b"\r\n");
        if n == 4 {
            puts(b"BS21 EMBASSY MULTITASK: PASS\r\n");
        }
    }
}

static EXECUTOR: StaticCell<Executor> = StaticCell::new();

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    // Start the TCXO free-running counter (the embassy-time `now()` source).
    let mut tcxo = hisi_riscv_hal::tcxo::TcxoDriver::new(p.TCXO);
    tcxo.enable();

    puts(b"\r\nBS21 embassy multitask (embassy-time: TCXO now() + TIMER alarm)\r\n");

    unsafe {
        core::arch::asm!("csrw mtvec, {0}", in(reg) atrap as *const () as usize); // direct mode
        interrupt::init();
        interrupt::enable_global();
    }

    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner: Spawner| {
        spawner.spawn(fast().unwrap());
        spawner.spawn(slow().unwrap());
    });
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
