//! # ws63-flashboot — EXPERIMENTAL second-stage bootloader for HiSilicon WS63
//!
//! ⚠️ **EXPERIMENTAL — NOT SECURE BOOT. NOT FOR PRODUCTION.** ⚠️
//!
//! This is a learning/experimental Rust rewrite of fbb_ws63
//! `flashboot_ws63/startup/main.c`. For production, use the vendor (fbb_ws63)
//! flashboot and run the Rust application in the partition it launches.
//! See `README.md` for the full rationale.
//!
//! Known gaps vs. the vendor bootloader (do NOT trust this as a root of trust):
//! - **NO authenticity check.** `verify_image_integrity()` only compares a SHA256
//!   of the body against `code_area_hash` stored in the *same unsigned header*. An
//!   attacker who can write flash recomputes the hash and boots arbitrary code at
//!   M-mode. The vendor does ECC-bp256/SM2 signature verification rooted in an
//!   efuse public key (`verify_image_head`/`verify_image_body`) — not done here.
//! - **No A/B slot selection / recovery / FOTA.** This boots the single primary app
//!   image. In production the running slot is chosen by the vendor's upg run-region
//!   config (magic `0x70746C6C` at the end of `PARTITION_FOTA_DATA`; `run_region`
//!   0=A/1=B) plus the partition table (@`0x200380`). Note `0x40000024` is the
//!   flashboot **self-recovery** flag (`0x5A5A5A5A` => restore the *bootloader* from
//!   its backup partition), NOT an app-slot selector — earlier code misused it as one.
//! - **Header layout** now mirrors `fbb_ws63/.../secure_verify_boot.h` (ECC256:
//!   `code_area_len`@CodeInfo+0x24, `code_area_hash`@+0x28), but is still not
//!   validated against a real signed vendor image on hardware.
//! - **Stubs:** `boot_clock_adapt()` is a TODO no-op, `read_partition_app_addr()`
//!   always returns `FLASH_START` (no partition-table parse), `check_upgrade_mode()`
//!   always returns false. No image decompression, no flash on-line encryption.
//!
//! Called by asm/startup.S as `flashboot_main()`.

#![no_std]
#![no_main]

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;

// Include assembly startup (runs before flashboot_main())
global_asm!(include_str!("../asm/startup.S"));

mod image;
mod sfc;
mod sha256;
mod uart;

// ── Peripherals (raw MMIO — intentionally independent of ws63-pac) ──

const HW_CTL: *const u32 = 0x4000_0014 as *const u32;
const CLDO_CRG_CLK_SEL: *mut u32 = 0x4400_1134 as *mut u32;
const CMU_NEW_CFG1: *mut u32 = 0x4000_34A4 as *mut u32;
const CLDO_CKEN_CTL1: *mut u32 = 0x4400_1104 as *mut u32;
const FAMA_REMAP: *mut u32 = 0x4400_7800 as *mut u32;
const WDT: *mut u32 = 0x4000_6000 as *mut u32;
const EFUSE_CTL: *mut u32 = 0x4400_8000 as *mut u32;
const EFUSE_CLK_PERIOD: *mut u32 = 0x4400_8004 as *mut u32;

// Boot flag saved by startup.S from a0 register
unsafe extern "C" {
    static __flash_boot_flag: u32;
}

const FLASH_START: u32 = 0x0020_0000;
const IMAGE_HEADER_LEN: u32 = 0x300;
/// Max app image size — the flash app window remapped for execution via FAMA.
const APP_MAX_SIZE: u32 = 0x0028_0000;

// ── Entry point (called from asm/startup.S) ────────────────────

/// Second-stage boot entry, called once from `asm/startup.S` after stack setup.
///
/// # Safety
/// Must be invoked exactly once at boot, from M-mode, by the startup assembly —
/// not callable from Rust. It does raw MMIO across the SoC and ultimately jumps
/// to the loaded app, so there is no valid state in which a second caller is sound.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn flashboot_main() -> ! {
    let tcxo_hz = if unsafe { HW_CTL.read_volatile() & 1 != 0 } {
        40_000_000
    } else {
        24_000_000
    };

    // P0: adapt UART/WDT/timer tick rates to TCXO frequency
    boot_clock_adapt(tcxo_hz);

    // P0: init efuse (clock period + chip type detect)
    efuse_init(tcxo_hz);

    // Flash → PLL (fbb_ws63: switch_flash_clock_to_pll)
    switch_flash_to_pll(tcxo_hz);

    // UART → PLL, then init for debug
    switch_uart_to_pll();
    uart::init(160_000_000, 115200);
    log("ws63-flashboot v0.2\n");

    // WDT: 65s timeout, reset on expiry
    wdg_init(65);

    // SFC: quad-SPI flash read
    if !sfc::sfc_init(tcxo_hz) {
        log("FAIL: SFC\n");
        halt();
    }

    // P1: upgrade mode check (fbb_ws63: ws63_upg_check)
    if check_upgrade_mode() {
        log("UPGRADE mode\n");
        enter_upgrade();
    }

    // FAMA: remap the flash app window → program execution address
    unsafe {
        let a = FLASH_START >> 12;
        let e = (FLASH_START + APP_MAX_SIZE) >> 12;
        FAMA_REMAP.write_volatile(a);
        FAMA_REMAP.add(8).write_volatile(e);
        FAMA_REMAP.add(16).write_volatile(0x230000 >> 12);
    }
    wdg_feed();

    // P1: locate the app image (partition-table lookup — stubbed; see fn).
    let img_addr = read_partition_app_addr();
    if img_addr == 0 {
        log("FATAL: no partition\n");
        halt();
    }

    // Single-image boot. We deliberately do NOT do A/B slot selection: the running
    // slot is chosen by the vendor's upg run-region config (magic 0x70746C6C at the
    // end of PARTITION_FOTA_DATA; run_region 0=A/1=B) + the partition table — which
    // this experimental loader does not parse. Address 0x40000024 is the flashboot
    // SELF-recovery flag (0x5A5A5A5A => restore the bootloader from its backup
    // partition), NOT an app-slot selector; the earlier code misused it as one.
    // Production A/B / recovery / FOTA is the vendor flashboot's job (see README).
    if try_boot(img_addr) { /* no return on success */ }

    log("FATAL: no valid image\n");
    halt();
}

// ── Boot region ─────────────────────────────────────────────────

fn try_boot(addr: u32) -> bool {
    log("App image: ");

    let hdr = sfc::read_image_header(addr);
    if !image::validate(&hdr) {
        uart::puts("invalid header\n");
        return false;
    }

    // P1: integrity check — SHA256 of the app body vs the header's code_area_hash.
    // This is NOT authenticity (the hash lives in the same unsigned header); see docs.
    let img_body = addr + IMAGE_HEADER_LEN;
    let img_len = hdr.code_info.code_area_len;
    let expected_hash = hdr.code_info.code_area_hash;
    if !verify_image_integrity(img_body, img_len, &expected_hash) {
        uart::puts("hash mismatch\n");
        return false;
    }

    let entry = addr + IMAGE_HEADER_LEN;
    log("jump to ");
    uart::puthex32(entry);
    uart::puts("\n");
    unsafe { asm!("csrw mie, zero", options(nomem, nostack)) };
    wdg_feed();
    // SAFETY: transmute is sound because:
    // 1. `entry` is a valid RISC-V function pointer (app binary entry at image_addr+0x300)
    // 2. Both `extern "C" fn() -> !` and `*const ()` have the same size (pointer-width on RV32)
    // 3. The app's entry point is compiled with the same ABI (RISC-V RV32IMFC ilp32f)
    let app: extern "C" fn() -> ! = unsafe { core::mem::transmute(entry as *const ()) };
    app();
}

// ── P0: Clock adapt ─────────────────────────────────────────────

fn boot_clock_adapt(tcxo_hz: u32) {
    // fbb_ws63: boot_clock_adapt() — sets UART/WDT/timer base clocks
    // to match detected TCXO frequency.
    //
    // UART PCLK = TCXO * multiplier (depends on CLDO_CRG divider).
    // For now, inform WDT of TCXO rate (WDT uses 32.768kHz always,
    // but the timeout calculation in WDT driver needs TCXO for delay).
    //
    // This is called BEFORE switch-uart-to-PLL — it configures
    // the TCXO-derived clock dividers while still on TCXO source.

    let tcxo_mhz = tcxo_hz / 1_000_000;
    let _ = tcxo_mhz; // used for divider config below

    // TODO: program CLDO_CRG dividers for UART/I2C/SPI based on TCXO rate
    // For 24MHz TCXO: UART needs ÷1 (~24MHz) or ÷N depending on target
    // For 40MHz TCXO: UART needs ÷1 or different divider
}

// ── P0: eFuse init ──────────────────────────────────────────────

fn efuse_init(tcxo_hz: u32) {
    // fbb_ws63: set_efuse_period() + uapi_efuse_init()
    unsafe {
        // Set efuse clock period to ~1us (TCXO_HZ / 1_000_000)
        let period = tcxo_hz / 1_000_000;
        EFUSE_CLK_PERIOD.write_volatile(period & 0xFF);

        // Enable efuse (write-read mode = 0)
        EFUSE_CTL.write_volatile(0);
    }
}

// ── P1: Partition table ─────────────────────────────────────────

fn read_partition_app_addr() -> u32 {
    // STUB — does NOT parse the partition table; returns the flash base.
    // The real lookup (fbb_ws63 uapi_partition_get_info(PARTITION_APP_IMAGE)) reads
    // the partition table at flash 0x200380 (header magic 0x4b87a54b + 16 entries of
    // addr/size/id) and returns the APP_IMAGE entry's flash address. Implementing it
    // here would require the partition-table layout + the upg run-region selection;
    // out of scope for this experimental loader (production uses the vendor flashboot).
    FLASH_START
}

// ── P1: Upgrade mode ────────────────────────────────────────────

fn check_upgrade_mode() -> bool {
    // fbb_ws63: ws63_upg_check() — checks if we should enter
    // firmware upgrade mode (e.g., GPIO pin held, magic flag in NV).
    // Returns false for normal boot.
    false // normal boot by default
}

fn enter_upgrade() -> ! {
    log("Entering upgrade mode...\n");
    // fbb_ws63: runs serial command loop for firmware update
    // Minimal: just halt
    halt();
}

// ── P1: SHA256 image INTEGRITY check (NOT authenticity) ─────────

fn verify_image_integrity(img_body: u32, img_len: u32, expected: &[u8; 32]) -> bool {
    // INTEGRITY only: SHA256(body) == code_area_hash from the header. This detects
    // accidental corruption, NOT tampering — the hash is in the same unsigned header,
    // so an attacker who can write flash recomputes it. The vendor authenticates via
    // ECC/SM2 signatures over the key/code areas rooted in an efuse key. SHA256 here
    // is the unaudited software impl in `sha256.rs` (integrity, not a security primitive).
    if img_len == 0 || img_len > 8 * 1024 * 1024 {
        return false;
    }

    // Read image body in 256-byte chunks, compute SHA256
    let mut sha = sha256::Sha256::new();
    let mut offset = 0u32;
    let mut buf = [0u8; 256];

    while offset < img_len {
        let chunk = core::cmp::min(256, (img_len - offset) as usize);
        sfc::read_bytes(img_body + offset, &mut buf[..chunk]);
        sha.update(&buf[..chunk]);
        offset += chunk as u32;
    }

    let hash = sha.finish();
    // Compare computed hash with expected hash from image header
    hash == *expected
}

// ── Hardware helpers ────────────────────────────────────────────

fn switch_flash_to_pll(tcxo_hz: u32) {
    unsafe {
        CMU_NEW_CFG1.write_volatile(0x1);
        delay((tcxo_hz / 1_000_000) / 3);
        CMU_NEW_CFG1.write_volatile(0x3);
        CLDO_CRG_CLK_SEL.write_volatile(CLDO_CRG_CLK_SEL.read_volatile() | (1 << 18));
    }
}

fn switch_uart_to_pll() {
    unsafe {
        let g = CLDO_CKEN_CTL1.read_volatile() & !(1 << 18);
        CLDO_CKEN_CTL1.write_volatile(g);
        CLDO_CRG_CLK_SEL.write_volatile(CLDO_CRG_CLK_SEL.read_volatile() | (1 << 1));
        CLDO_CKEN_CTL1.write_volatile(g | (1 << 18));
    }
}

fn wdg_init(timeout_s: u32) {
    unsafe {
        WDT.write_volatile(0x5A5A5A5A);
        WDT.add(1).write_volatile((timeout_s * 32768) << 8);
        WDT.add(4)
            .write_volatile(0x01 | (1 << 2) | (7 << 3) | (1 << 6));
        WDT.write_volatile(0);
    }
}

fn wdg_feed() {
    unsafe {
        WDT.write_volatile(0x5A5A5A5A);
        WDT.add(2).write_volatile(1);
        WDT.write_volatile(0);
    }
}

fn delay(loops: u32) {
    for _ in 0..loops {
        unsafe { asm!("nop", options(nomem, nostack)) };
    }
}

fn log(msg: &str) {
    uart::puts(msg);
}

fn halt() -> ! {
    loop {
        unsafe { asm!("wfi", options(nomem, nostack)) };
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    log("PANIC\n");
    halt();
}
