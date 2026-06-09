//! UAPI platform services (ws63-RF `port_uapi.h`).
//!
//! `uapi_systick_get_ms` is real (reads the RISC-V `mcycle` counter).
//! `uapi_tsensor_get_current_temp` returns a fixed safe value and `uapi_nv_read`
//! is a stub — both need hisi-riscv-hal tsensor / a flash-NV backing (phase 4): the
//! real `uapi_nv_read` returns calibrated RF parameters + the MAC address from
//! flash/eFuse, without which the RF front-end cannot be calibrated.

// C-ABI entry points: the blob passes valid pointers; the safety contract is
// the C signature, not a Rust `unsafe` marker.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

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
/// algorithms read this; a real reading needs the hisi-riscv-hal tsensor — phase 4).
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

/// Write an item to non-volatile storage. STUB: accepted, not persisted.
#[unsafe(no_mangle)]
pub extern "C" fn uapi_nv_write(_id: u32, _buf: *const c_void, _len: u32) -> i32 {
    crate::OSAL_OK
}

// ── eFuse / TRNG / device identity ───────────────────────────────────────────
// These feed RF calibration, the MAC address and crypto seeding. They are
// SCAFFOLD values good enough to LINK and to bring the stack up under emulation;
// a hardware run must source real eFuse/TRNG via hisi-riscv-hal (phase 4).

/// One eFuse bit. STUB: always 0.
#[unsafe(no_mangle)]
pub extern "C" fn uapi_efuse_read_bit(value: *mut u8, _byte: u32, _bit: u8) -> u32 {
    if !value.is_null() {
        // SAFETY: valid out-parameter.
        unsafe { *value = 0 };
    }
    crate::OSAL_OK as u32
}

/// A run of eFuse bytes. STUB: zero-filled.
#[unsafe(no_mangle)]
pub extern "C" fn uapi_efuse_read_buffer(buffer: *mut u8, _byte: u32, length: u16) -> u32 {
    if !buffer.is_null() {
        // SAFETY: caller guarantees `length` bytes.
        unsafe { core::ptr::write_bytes(buffer, 0, length as usize) };
    }
    crate::OSAL_OK as u32
}

/// Random bytes. SCAFFOLD: a tiny `mcycle`-seeded xorshift (NOT cryptographically
/// secure — a hardware run must use the real TRNG via hisi-riscv-hal).
#[unsafe(no_mangle)]
pub extern "C" fn uapi_drv_cipher_trng_get_random_bytes(randnum: *mut u8, size: u32) -> u32 {
    if randnum.is_null() {
        return crate::OSAL_NOK as u32;
    }
    let mut state = read_mcycle() | 1;
    for i in 0..size as usize {
        // xorshift64
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        // SAFETY: caller guarantees `size` bytes.
        unsafe { *randnum.add(i) = (state & 0xff) as u8 };
    }
    crate::OSAL_OK as u32
}

/// Device address (e.g. station MAC). SCAFFOLD: a fixed locally-administered MAC
/// `02:00:00:00:00:01` (`type`/`len` ignored). A real device reads it from
/// eFuse/NV. Returns `OSAL_OK`.
#[unsafe(no_mangle)]
pub extern "C" fn get_dev_addr(pc_addr: *mut u8, addr_len: u8, _type: u8) -> u32 {
    if pc_addr.is_null() || addr_len == 0 {
        return crate::OSAL_NOK as u32;
    }
    const MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
    let n = (addr_len as usize).min(MAC.len());
    // SAFETY: caller guarantees `addr_len` bytes.
    unsafe { core::ptr::copy_nonoverlapping(MAC.as_ptr(), pc_addr, n) };
    crate::OSAL_OK as u32
}

/// TCXO reference frequency in Hz. SCAFFOLD: 24 MHz (the WS63 nominal; matches
/// the ws63-qemu clock model).
#[unsafe(no_mangle)]
pub extern "C" fn get_tcxo_freq() -> u32 {
    24_000_000
}

// ── AT command console (not wired — the runtime owns the console) ────────────

/// Register a BT AT command table. STUB: ignored.
#[unsafe(no_mangle)]
pub extern "C" fn uapi_at_bt_register_cmd(_table: *const c_void, _num: u16) -> u32 {
    crate::OSAL_OK as u32
}

/// AT console print. STUB: ignored (the runtime owns the console).
#[unsafe(no_mangle)]
pub extern "C" fn uapi_at_print(_fmt: *const core::ffi::c_char) -> u32 {
    crate::OSAL_OK as u32
}

// ── Wi-Fi service entry points referenced internally ─────────────────────────

/// Stop the SoftAP. STUB.
#[unsafe(no_mangle)]
pub extern "C" fn uapi_wifi_softap_stop() -> i32 {
    crate::OSAL_OK
}

/// Stop the station. STUB.
#[unsafe(no_mangle)]
pub extern "C" fn uapi_wifi_sta_stop() -> i32 {
    crate::OSAL_OK
}
