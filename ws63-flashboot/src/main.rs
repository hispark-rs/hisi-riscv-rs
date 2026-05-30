//! # ws63-flashboot — Second-stage bootloader for HiSilicon WS63
//!
//! Ported from fbb_ws63 C SDK (`flashboot_ws63/startup/main.c`).
//! This is the first code that runs after the mask ROM. It:
//!
//! 1. Detects TCXO crystal frequency (24 or 40 MHz)
//! 2. Initializes the SPI Flash Controller (SFC) for quad-SPI read
//! 3. Configures the FAMA address remap for the application image
//! 4. Reads and validates the application image header
//! 5. Jumps to the application entry point
//!
//! # Image layout on flash
//!
//! ```text
//! +===================+ <- image_addr (partition start)
//! | key_area  (0x100) |    signature + public key
//! +-------------------+
//! | code_info (0x200) |    image_id, version, hash, length
//! +===================+ <- app_entry = image_addr + 0x300
//! | app binary        |    RISC-V .text + .rodata + .data + ...
//! +-------------------+
//! ```
//!
//! # Memory map
//!
//! | Region | Address | Purpose |
//! |--------|---------|---------|
//! | PROGRAM | 0x230300 | Flashboot code in SPI flash (XIP) |
//! | FLASHBOOT_RAM | 0xA28000 | 32KB SRAM for stack + BSS |
//! | FLASH_START | 0x200000 | SPI flash mapping base |

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

mod sfc;
mod image;

// ── Register addresses (from fbb_ws63) ──────────────────────────

/// TCXO frequency detect: bit[0] = 1 → 40MHz, 0 → 24MHz
const HW_CTL: *const u32 = 0x4000_0014 as *const u32;
/// CLDO_CRG clock select register
const CLDO_CRG_CLK_SEL: *mut u32 = 0x4400_1134 as *mut u32;
/// CMU flash clock control
const CMU_NEW_CFG1: *mut u32 = 0x4000_34A4 as *mut u32;
/// CLDO_SUB_CRG clock enable control 1 (UART clock gates)
const CLDO_SUB_CRG_CKEN_CTL1: *mut u32 = 0x4400_1104 as *mut u32;
/// FAMA_REMAP base address
const FAMA_REMAP_BASE: *mut u32 = 0x4400_7800 as *mut u32;
/// Flash boot type register
const FLASH_BOOT_TYPE_REG: *const u32 = 0x4000_0024 as *const u32;
/// SPI flash mapping start address
const FLASH_START: u32 = 0x0020_0000;

// ── Constants from fbb_ws63 ─────────────────────────────────────

const APP_START_INSTRUCTION: u32 = 0x0040_006F; // lui x0, 0x400; jal x0, entry
const IMAGE_HEADER_LEN: u32 = 0x300;             // key_area(0x100) + code_info(0x200)
const FLASH_BOOT_MAIN: u32 = 0xA5A5_A5A5;
const FLASH_BOOT_BKUP: u32 = 0x5A5A_5A5A;
const DELAY_1_US_K: u32 = 8;                     // ~1µs delay loop count at 240MHz
const WDT_BASE: *mut u32 = 0x4000_6000 as *mut u32;

// ── Entry point ─────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn main() -> ! {
    // Step 1: Detect TCXO frequency
    let tcxo_40mhz = unsafe { HW_CTL.read_volatile() & 1 != 0 };
    let tcxo_hz: u32 = if tcxo_40mhz { 40_000_000 } else { 24_000_000 };

    // Steps 2-3: Switch flash clock to PLL (fbb_ws63: switch_flash_clock_to_pll)
    unsafe {
        CMU_NEW_CFG1.write_volatile(0x1); // CPU_DIV_FLASH_RSTN_SYNC
        delay_us(1, tcxo_hz);
        CMU_NEW_CFG1.write_volatile(0x3); // CPU_DIV_FLASH_RSTN
        let val = CLDO_CRG_CLK_SEL.read_volatile();
        CLDO_CRG_CLK_SEL.write_volatile(val | (1 << 18)); // flash → PLL
    }

    // Step 4: Switch UART0 clock to PLL (for debug output)
    unsafe {
        let mut gate = CLDO_SUB_CRG_CKEN_CTL1.read_volatile();
        gate &= !(1 << 18);                     // disable UART0 gate
        CLDO_SUB_CRG_CKEN_CTL1.write_volatile(gate);
        let sel = CLDO_CRG_CLK_SEL.read_volatile();
        CLDO_CRG_CLK_SEL.write_volatile(sel | (1 << 1)); // UART0 → PLL
        gate |= 1 << 18;                        // re-enable UART0 gate
        CLDO_SUB_CRG_CKEN_CTL1.write_volatile(gate);
    }

    // Step 5: Initialize watchdog (65s timeout, fbb_ws63: boot_wdt_init)
    unsafe {
        // Unlock WDT
        let wdt_lock = WDT_BASE;
        wdt_lock.write_volatile(0x5A5A5A5A);
        // Load = 65s * 32768 ≈ 2,129,920 = 0x208000
        let wdt_load = WDT_BASE.add(1);
        wdt_load.write_volatile(0x208000 << 8);
        // Control: enable, reset on timeout, pulse=256 cycles
        let wdt_cr = WDT_BASE.add(4);
        wdt_cr.write_volatile(0x01 | (1 << 2) | (7 << 3) | (1 << 6));
        // Lock
        wdt_lock.write_volatile(0);
    }

    // Step 6: Initialize SFC flash (quad-SPI read mode)
    if !sfc::sfc_init(tcxo_hz) {
        panic_loop();
    }

    // Step 7: Configure FAMA address remap for app image
    // (fbb_ws63: dmmu_set — maps flash address to program region)
    unsafe {
        // Region 0: remap program region to flash start
        // FAMA_REMAP_SRC + 0*4, FAMA_REMAP_LEN + 0*4, FAMA_REMAP_DST + 0*4
        let app_start = FLASH_START >> 12; // 0x200
        let app_size  = 0x280000 >> 12;     // 2.5MB in 4KB pages
        let dst_start = 0x230000 >> 12;     // PROGRAM base in 4KB pages
        FAMA_REMAP_BASE.write_volatile(app_start);                   // src
        FAMA_REMAP_BASE.add(0x20 / 4).write_volatile(app_start + app_size); // src end
        FAMA_REMAP_BASE.add(0x40 / 4).write_volatile(dst_start);     // dst
    }

    // Step 8: Feed watchdog before image operations
    unsafe {
        let wdt_lock = WDT_BASE;
        wdt_lock.write_volatile(0x5A5A5A5A);
        let wdt_restart = WDT_BASE.add(2);
        wdt_restart.write_volatile(1);
        wdt_lock.write_volatile(0);
    }

    // Step 9: Read boot type and locate app partition
    let run_region = unsafe { read_boot_type() }; // 0 = region A, 1 = region B
    let partition_offset: u32 = if run_region == 0 {
        0x0000_0000     // Region A: start of app area
    } else {
        0x0028_0000     // Region B: ~2.5MB offset (example)
    };

    // Step 10: Read image header from flash
    let image_addr = FLASH_START + partition_offset;
    let header = sfc::read_image_header(image_addr);

    // Step 11: Validate image header
    if !image::validate_header(&header) {
        // Try backup region
        let backup_offset: u32 = if run_region == 0 { 0x0028_0000 } else { 0x0000_0000 };
        let backup_addr = FLASH_START + backup_offset;
        let backup_header = sfc::read_image_header(backup_addr);
        if !image::validate_header(&backup_header) {
            panic_loop();
        }
        // Use backup
        jump_to_app(backup_addr + IMAGE_HEADER_LEN);
    } else {
        jump_to_app(image_addr + IMAGE_HEADER_LEN);
    }
}

// ── Boot type detection ──────────────────────────────────────────

unsafe fn read_boot_type() -> u32 {
    let reg = unsafe { FLASH_BOOT_TYPE_REG.read_volatile() };
    if reg == FLASH_BOOT_MAIN {
        0 // Boot from main (region A)
    } else if reg == FLASH_BOOT_BKUP {
        1 // Boot from backup (region B)
    } else {
        0 // Default to region A
    }
}

// ── Jump to application ──────────────────────────────────────────

fn jump_to_app(entry_addr: u32) -> ! {
    // Disable interrupts before jump
    unsafe { asm!("csrw mie, zero", options(nomem, nostack)) };

    // Feed watchdog one last time
    unsafe {
        let wdt_lock = WDT_BASE;
        wdt_lock.write_volatile(0x5A5A5A5A);
        WDT_BASE.add(2).write_volatile(1);
        wdt_lock.write_volatile(0);
    }

    // Jump to application entry point (fbb_ws63: jump_to_execute_addr)
    let app_entry: extern "C" fn() -> ! = unsafe { core::mem::transmute(entry_addr as *const ()) };
    app_entry();
}

// ── Delay ────────────────────────────────────────────────────────

fn delay_us(us: u32, clk_hz: u32) {
    let cycles = clk_hz / 1_000_000 * us / DELAY_1_US_K;
    for _ in 0..cycles {
        unsafe { asm!("nop", options(nomem, nostack)) };
    }
}

// ── Panic handler ────────────────────────────────────────────────

fn panic_loop() -> ! {
    loop {
        unsafe { asm!("wfi", options(nomem, nostack)) };
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    panic_loop();
}
