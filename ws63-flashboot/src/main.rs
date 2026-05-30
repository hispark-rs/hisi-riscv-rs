//! # ws63-flashboot — Second-stage bootloader for HiSilicon WS63
//!
//! Rust rewrite of fbb_ws63 `flashboot_ws63/startup/main.c`.
//! Standalone — no HAL dependency. Registers accessed via raw pointers.
//!
//! ## Boot sequence: clock → WDT → SFC → FAMA → partition → verify → jump
//!
//! | Region | Address | Purpose |
//! |--------|---------|---------|
//! | PROGRAM | 0x230300 | Flashboot code in SPI flash (XIP) |
//! | FLASHBOOT_RAM | 0xA28000 | 32KB SRAM for stack + data |

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

mod image;
mod sfc;
mod sha256;
mod uart;

// ── Peripherals (raw MMIO — intentionally independent of ws63-pac) ──

const HW_CTL: *const u32            = 0x4000_0014 as *const u32; // TCXO detect
const CLDO_CRG_CLK_SEL: *mut u32    = 0x4400_1134 as *mut u32; // clock select
const CMU_NEW_CFG1: *mut u32        = 0x4000_34A4 as *mut u32; // flash clock
const CLDO_CKEN_CTL1: *mut u32      = 0x4400_1104 as *mut u32; // UART gate
const FAMA_REMAP: *mut u32          = 0x4400_7800 as *mut u32; // addr remap
const FLASH_BOOT_TYPE: *const u32   = 0x4000_0024 as *const u32;
const WDT: *mut u32                 = 0x4000_6000 as *mut u32; // WDT base

const FLASH_START: u32       = 0x0020_0000;
const IMAGE_HEADER_LEN: u32  = 0x300;
const BOOT_MAIN: u32         = 0xA5A5_A5A5;
const REGION_SIZE: u32       = 0x0028_0000;
const REGION_OFFSETS: [(u32, char); 2] = [(0, 'A'), (REGION_SIZE, 'B')];

// ── Entry point ─────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn main() -> ! {
    let tcxo_hz = if unsafe { HW_CTL.read_volatile() & 1 != 0 } { 40_000_000 } else { 24_000_000 };

    // Clock: flash + UART → PLL (duplicated from hal/clock_init.rs — flashboot runs first)
    switch_flash_to_pll(tcxo_hz);
    switch_uart_to_pll();
    uart::init(160_000_000, 115200);

    log("ws63-flashboot v0.1\n");

    // WDT: 65s timeout, reset on expiry
    wdg_init(65);

    // SFC: quad-SPI flash read
    if !sfc::sfc_init(tcxo_hz) { log("FAIL: SFC\n"); halt(); }

    // FAMA: remap flash→program region
    unsafe {
        let a = FLASH_START >> 12;
        let e = (FLASH_START + REGION_SIZE * 2) >> 12;
        let d = 0x230000 >> 12;
        FAMA_REMAP.write_volatile(a);
        FAMA_REMAP.add(8).write_volatile(e);  // +0x20
        FAMA_REMAP.add(16).write_volatile(d); // +0x40
    }

    wdg_feed();

    // Partition scan: try primary → fallback → halt
    let boot_type = unsafe { FLASH_BOOT_TYPE.read_volatile() };
    let region: usize = if boot_type == BOOT_MAIN { 0 } else { 1 };
    let (pri, bak) = (region, 1 - region);

    if !try_boot(REGION_OFFSETS[pri].0, REGION_OFFSETS[pri].1) {
        log("primary invalid, trying backup\n");
        if !try_boot(REGION_OFFSETS[bak].0, REGION_OFFSETS[bak].1) {
            log("FATAL: no valid image\n");
        }
    }
    halt();
}

// ── Boot region ─────────────────────────────────────────────────

fn try_boot(offset: u32, name: char) -> bool {
    let addr = FLASH_START + offset;
    log("Region "); uart::putc(name as u8); uart::puts(": ");

    let hdr = sfc::read_image_header(addr);
    if !image::validate(&hdr) {
        uart::puts("invalid header\n");
        return false;
    }

    // Minimal verify: signature must exist
    if hdr.code_info.signature_length == 0 || hdr.code_info.signature_length > 512 {
        uart::puts("bad signature\n");
        return false;
    }

    // Jump
    let entry = addr + IMAGE_HEADER_LEN;
    log("jump to "); uart::puthex32(entry); uart::puts("\n");
    unsafe { asm!("csrw mie, zero", options(nomem, nostack)) };
    wdg_feed();
    let app: extern "C" fn() -> ! = unsafe { core::mem::transmute(entry as *const ()) };
    app();
}

// ── Hardware helpers (intentionally duplicated from HAL — boot-time only) ──

fn switch_flash_to_pll(tcxo_hz: u32) {
    // fbb_ws63: switch_flash_clock_to_pll() — CMU_NEW_CFG1 + CLDO_CRG_CLK_SEL bit 18
    unsafe {
        CMU_NEW_CFG1.write_volatile(0x1);
        delay(tcxo_hz / 1_000_000);
        CMU_NEW_CFG1.write_volatile(0x3);
        CLDO_CRG_CLK_SEL.write_volatile(CLDO_CRG_CLK_SEL.read_volatile() | (1 << 18));
    }
}

fn switch_uart_to_pll() {
    // fbb_ws63: switch_clock() — gate off → set bit 1 → gate on
    unsafe {
        let g = CLDO_CKEN_CTL1.read_volatile() & !(1 << 18);
        CLDO_CKEN_CTL1.write_volatile(g);
        CLDO_CRG_CLK_SEL.write_volatile(CLDO_CRG_CLK_SEL.read_volatile() | (1 << 1));
        CLDO_CKEN_CTL1.write_volatile(g | (1 << 18));
    }
}

fn wdg_init(timeout_s: u32) {
    unsafe {
        WDT.write_volatile(0x5A5A5A5A);                    // unlock
        WDT.add(1).write_volatile((timeout_s * 32768) << 8); // load
        WDT.add(4).write_volatile(0x01 | (1 << 2) | (7 << 3) | (1 << 6)); // en+rst
        WDT.write_volatile(0);                                // lock
    }
}

fn wdg_feed() {
    unsafe { WDT.write_volatile(0x5A5A5A5A); WDT.add(2).write_volatile(1); WDT.write_volatile(0); }
}

fn delay(loops: u32) {
    for _ in 0..loops / 3 { unsafe { asm!("nop", options(nomem, nostack)) }; }
}

fn log(msg: &str) { uart::puts(msg); }

fn halt() -> ! { loop { unsafe { asm!("wfi", options(nomem, nostack)) }; } }

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! { log("PANIC\n"); halt(); }