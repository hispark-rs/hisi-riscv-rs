//! UAPI platform services (ws63-RF `port_uapi.h`).
//!
//! `uapi_systick_get_ms` is real (reads the RISC-V `mcycle` counter).
//! `uapi_tsensor_get_current_temp` returns a fixed safe value and `uapi_nv_read`
//! is a stub — both need ws63-hal tsensor / a flash-NV backing (phase 4): the
//! real `uapi_nv_read` returns calibrated RF parameters + the MAC address from
//! flash/eFuse, without which the RF front-end cannot be calibrated.

use core::ffi::c_void;

/// Same rough cycles/µs as [`crate::osal`]; `mcycle / (CYCLES_PER_US*1000)` ≈ ms.
const CYCLES_PER_MS: u64 = 240 * 1000;

/// Milliseconds since boot, from the `mcycle` CSR (approximate — uncalibrated).
#[unsafe(no_mangle)]
pub extern "C" fn uapi_systick_get_ms() -> u64 {
    read_mcycle() / CYCLES_PER_MS
}

#[cfg(target_arch = "riscv32")]
fn read_mcycle() -> u64 {
    loop {
        let hi1: u32;
        let lo: u32;
        let hi2: u32;
        // SAFETY: reading performance CSRs; re-read hi to guard the low rollover.
        unsafe {
            core::arch::asm!("csrr {0}, mcycleh", out(reg) hi1, options(nomem, nostack));
            core::arch::asm!("csrr {0}, mcycle",  out(reg) lo,  options(nomem, nostack));
            core::arch::asm!("csrr {0}, mcycleh", out(reg) hi2, options(nomem, nostack));
        }
        if hi1 == hi2 {
            return ((hi1 as u64) << 32) | (lo as u64);
        }
    }
}
#[cfg(not(target_arch = "riscv32"))]
fn read_mcycle() -> u64 {
    0
}

/// Current chip temperature in °C. SCAFFOLD: fixed 25 °C (thermal-protection
/// algorithms read this; a real reading needs the ws63-hal tsensor — phase 4).
#[unsafe(no_mangle)]
pub extern "C" fn uapi_tsensor_get_current_temp() -> i32 {
    25
}

/// Read an item from non-volatile storage. STUB: returns failure (no NV
/// backing). The blob uses this for calibrated RF params / MAC address; until a
/// flash-NV source is wired (phase 4) the RF front-end stays uncalibrated.
#[unsafe(no_mangle)]
pub extern "C" fn uapi_nv_read(_id: u32, _buf: *mut c_void, _len: u32) -> i32 {
    crate::OSAL_NOK
}
