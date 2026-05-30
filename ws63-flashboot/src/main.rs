//! # ws63-flashboot — Second-stage bootloader for HiSilicon WS63
//!
//! Rust rewrite of fbb_ws63 `flashboot_ws63/startup/main.c`.
//!
//! ## Boot sequence
//!
//! 1. Clock init — TCXO detect + flash UART→PLL + WDT
//! 2. SFC init — quad-SPI flash read
//! 3. Partition scanning — A/B region detection
//! 4. Image header read + validate
//! 5. SHA256 hash verify (optional, image-dependent)
//! 6. Jump to app entry
//!
//! ## Memory map
//!
//! | Region | Address | Purpose |
//! |--------|---------|---------|
//! | PROGRAM | 0x230300 | Flashboot code in SPI flash (XIP) |
//! | FLASHBOOT_RAM | 0xA28000 | 32KB SRAM for stack + data |
//! | FLASH_START | 0x0020_0000 | SPI flash mapping base |

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

mod image;
mod sfc;
mod sha256;
mod uart;

// ── Register addresses (from fbb_ws63) ──────────────────────────

const HW_CTL: *const u32 = 0x4000_0014 as *const u32;
const CLDO_CRG_CLK_SEL: *mut u32 = 0x4400_1134 as *mut u32;
const CMU_NEW_CFG1: *mut u32 = 0x4000_34A4 as *mut u32;
const CLDO_SUB_CRG_CKEN_CTL1: *mut u32 = 0x4400_1104 as *mut u32;
const FAMA_REMAP_BASE: *mut u32 = 0x4400_7800 as *mut u32;
const FLASH_BOOT_TYPE_REG: *const u32 = 0x4000_0024 as *const u32;
const FLASH_START: u32 = 0x0020_0000;
const WDT_BASE: *mut u32 = 0x4000_6000 as *mut u32;

// ── Constants ───────────────────────────────────────────────────

const IMAGE_HEADER_LEN: u32 = 0x300;
const FLASH_BOOT_MAIN: u32 = 0xA5A5_A5A5;
const FLASH_BOOT_BKUP: u32 = 0x5A5A_5A5A;
const DELAY_1_US_K: u32 = 8;
const WDT_TIMEOUT_S: u32 = 65;
const UART_PCLK: u32 = 160_000_000;
const UART_BAUD: u32 = 115200;

// ── A/B partition offsets (from fbb_ws63 partition table) ───────

const REGION_A_OFFSET: u32 = 0x0000_0000;
const REGION_B_OFFSET: u32 = 0x0028_0000;
const REGION_SIZE: u32 = 0x0028_0000; // 2.5MB per region

// ── Entry point ─────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn main() -> ! {
    // ── Step 1: Clock init ──────────────────────────────────
    let tcxo_40mhz = unsafe { HW_CTL.read_volatile() & 1 != 0 };
    let tcxo_hz: u32 = if tcxo_40mhz { 40_000_000 } else { 24_000_000 };

    // Switch flash to PLL
    unsafe {
        CMU_NEW_CFG1.write_volatile(0x1);
        delay_us(1, tcxo_hz);
        CMU_NEW_CFG1.write_volatile(0x3);
        CLDO_CRG_CLK_SEL.write_volatile(CLDO_CRG_CLK_SEL.read_volatile() | (1 << 18));
    }

    // Switch UART0→PLL, then init UART for debug
    unsafe {
        let gate = CLDO_SUB_CRG_CKEN_CTL1.read_volatile() & !(1 << 18);
        CLDO_SUB_CRG_CKEN_CTL1.write_volatile(gate);
        CLDO_CRG_CLK_SEL.write_volatile(CLDO_CRG_CLK_SEL.read_volatile() | (1 << 1));
        CLDO_SUB_CRG_CKEN_CTL1.write_volatile(gate | (1 << 18));
    }
    uart::init(UART_PCLK, UART_BAUD);

    boot_log("ws63-flashboot v0.1.0");
    boot_log("TCXO: ");
    uart::puthex32(tcxo_hz);
    uart::puts(" Hz\n");

    // ── Step 2: WDT init ────────────────────────────────────
    boot_log("WDT: init\n");
    unsafe {
        WDT_BASE.write_volatile(0x5A5A5A5A);
        let load = WDT_TIMEOUT_S * 32768;
        WDT_BASE.add(1).write_volatile(load << 8);
        WDT_BASE.add(4).write_volatile(0x01 | (1 << 2) | (7 << 3) | (1 << 6));
        WDT_BASE.write_volatile(0);
    }

    // ── Step 3: SFC init ────────────────────────────────────
    boot_log("SFC: init\n");
    if !sfc::sfc_init(tcxo_hz) {
        boot_log("FAIL: SFC init\n");
        hang();
    }

    // ── Step 4: FAMA remap ──────────────────────────────────
    boot_log("FAMA: remap\n");
    unsafe {
        let app_base = FLASH_START >> 12;
        let app_end = (FLASH_START + REGION_SIZE * 2) >> 12;
        let dst_base = 0x230000 >> 12;
        FAMA_REMAP_BASE.write_volatile(app_base);
        FAMA_REMAP_BASE.add(0x20 / 4).write_volatile(app_end);
        FAMA_REMAP_BASE.add(0x40 / 4).write_volatile(dst_base);
    }

    // ── Step 5: Feed watchdog ───────────────────────────────
    feed_wdt();

    // ── Step 6: Partition scan ──────────────────────────────
    let boot_type = unsafe { FLASH_BOOT_TYPE_REG.read_volatile() };
    let run_region: u32 = if boot_type == FLASH_BOOT_MAIN { 0 } else { 1 };
    let regions = [(REGION_A_OFFSET, 'A'), (REGION_B_OFFSET, 'B')];

    // Try primary region first, fall back to backup
    let (pri_offset, pri_name) = regions[run_region as usize];
    let (bak_offset, bak_name) = regions[1 - run_region as usize];

    if try_boot_region(pri_offset, pri_name, tcxo_hz) {
        // will not return
    }

    boot_log("Primary region ");
    uart::putc(pri_name as u8);
    uart::puts(" invalid, trying backup\n");

    if try_boot_region(bak_offset, bak_name, tcxo_hz) {
        // will not return
    }

    // Both regions failed
    boot_log("FATAL: no valid image found\n");
    hang();
}

// ── Boot region attempt ──────────────────────────────────────────

fn try_boot_region(offset: u32, name: char, tcxo_hz: u32) -> bool {
    let image_addr = FLASH_START + offset;

    boot_log("Region ");
    uart::putc(name as u8);
    uart::puts(": addr=");
    uart::puthex32(image_addr);
    uart::puts("\n");

    // Read image header
    let header = sfc::read_image_header(image_addr);

    // Validate header
    if !image::validate_header(&header) {
        boot_log("Region ");
        uart::putc(name as u8);
        uart::puts(": invalid header\n");
        return false;
    }

    boot_log("Region ");
    uart::putc(name as u8);
    uart::puts(": image_id=");
    uart::puthex32(header.code_info.image_id);
    uart::puts(" len=");
    uart::puthex32(header.code_info.image_length);
    uart::puts("\n");

    // Verify image hash if available
    let hash_ok = verify_image_hash(image_addr, &header, tcxo_hz);
    if !hash_ok {
        boot_log("Region ");
        uart::putc(name as u8);
        uart::puts(": hash mismatch\n");
        return false;
    }

    // Jump to app
    boot_log("Jump to ");
    uart::putc(name as u8);
    uart::puts(": ");
    uart::puthex32(image_addr + IMAGE_HEADER_LEN);
    uart::puts("\n");

    jump_to_app(image_addr + IMAGE_HEADER_LEN);
}

// ── SHA256 image verification ────────────────────────────────────

fn verify_image_hash(image_addr: u32, header: &sfc::ImageHeader, _tcxo_hz: u32) -> bool {
    let img_len = header.code_info.image_length;
    if img_len == 0 {
        return false;
    }

    // For production: compute SHA256 of app binary and compare with
    // header.code_info hash field. This requires reading the full
    // app binary from flash, which is slow in software.
    //
    // For now, verify the header signature is non-trivial
    // (fbb_ws63 does full ECC/SM2 signature verification via ROM).

    let sig_len = header.code_info.signature_length;
    if sig_len == 0 {
        return false; // unsigned images not allowed
    }

    // Minimal check: signature length is reasonable
    if sig_len > 512 {
        return false;
    }

    // TODO: full SHA256 computation of image body
    // let mut sha = sha256::Sha256::new();
    // Read image in chunks, update SHA, compare hash

    true // accept for now (signature exists)
}

// ── Jump to application ──────────────────────────────────────────

fn jump_to_app(entry_addr: u32) -> ! {
    boot_log("Disabling interrupts, jumping...\n");

    // Disable interrupts
    unsafe { asm!("csrw mie, zero", options(nomem, nostack)) };

    // Feed watchdog one last time
    feed_wdt();

    // Flush UART
    for _ in 0..10000 {
        unsafe { asm!("nop", options(nomem, nostack)) };
    }

    // Jump
    let app_entry: extern "C" fn() -> ! =
        unsafe { core::mem::transmute(entry_addr as *const ()) };
    app_entry();
}

// ── Utilities ────────────────────────────────────────────────────

fn feed_wdt() {
    unsafe {
        WDT_BASE.write_volatile(0x5A5A5A5A);
        WDT_BASE.add(2).write_volatile(1);
        WDT_BASE.write_volatile(0);
    }
}

fn delay_us(us: u32, clk_hz: u32) {
    let cycles = clk_hz / 1_000_000 * us / DELAY_1_US_K;
    for _ in 0..cycles {
        unsafe { asm!("nop", options(nomem, nostack)) };
    }
}

fn boot_log(msg: &str) {
    uart::puts("[fb] ");
    uart::puts(msg);
}

// ── Panic / hang ─────────────────────────────────────────────────

fn hang() -> ! {
    boot_log("HALT\n");
    loop {
        unsafe { asm!("wfi", options(nomem, nostack)) };
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    boot_log("PANIC: ");
    // no_std: can't format panic message, just indicate
    let _ = info.message();
    uart::puts("core panic\n");
    hang();
}