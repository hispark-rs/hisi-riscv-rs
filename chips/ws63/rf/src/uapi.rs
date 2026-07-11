//! UAPI platform services (ws63-RF `port_uapi.h`).
//!
//! `uapi_systick_get_ms` is real (reads the RISC-V `mcycle` counter).
//! `uapi_nv_read` is backed by the official WS63 ACPU KV partition and validates
//! its page/key metadata and CRC. `uapi_tsensor_get_current_temp` remains a fixed
//! conservative value until the HAL sensor path is wired into the RF adapter.

// C-ABI entry points: the blob passes valid pointers; the safety contract is
// the C signature, not a Rust `unsafe` marker.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::c_void;
use portable_atomic::{AtomicBool, Ordering};

static EFUSE_READY: AtomicBool = AtomicBool::new(false);

#[cfg(target_arch = "riscv32")]
pub(crate) fn enable_efuse_reads() {
    EFUSE_READY.store(true, Ordering::Release);
}

/// Same rough cycles/µs as [`crate::osal`]; `mcycle / (CYCLES_PER_US*1000)` ≈ ms.
const CYCLES_PER_MS: u64 = 240 * 1000;

fn trace_nv(key: u16, max_len: u16, actual_len: u16, result: u32) {
    #[cfg(feature = "rf-init-diag")]
    crate::rf_init_diag::trace_nv(key, max_len, actual_len, result);
    #[cfg(not(feature = "rf-init-diag"))]
    let _ = (key, max_len, actual_len, result);
}

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

/// Current chip temperature in °C.
///
/// SCAFFOLD: writes a conservative 25 °C. The pointer/result ABI matches the
/// vendor SDK; a real reading still needs the hisi-riscv-hal tsensor (RF2/RF3).
#[unsafe(no_mangle)]
pub extern "C" fn uapi_tsensor_get_current_temp(temp: *mut i8) -> u32 {
    if temp.is_null() {
        return crate::OSAL_NOK as u32;
    }
    // SAFETY: the SDK ABI defines `temp` as a writable one-byte out-parameter.
    unsafe { *temp = 25 };
    crate::OSAL_OK as u32
}

/// Read a plaintext item from the official WS63 ACPU KV partition.
///
/// Encrypted records are rejected until the device crypto-key path is wired.
#[unsafe(no_mangle)]
pub extern "C" fn uapi_nv_read(
    _key: u16,
    _max_len: u16,
    actual_len: *mut u16,
    _value: *mut u8,
) -> u32 {
    if !actual_len.is_null() {
        // SAFETY: the SDK ABI defines this as a writable out-parameter.
        unsafe { *actual_len = 0 };
    }

    #[cfg(target_arch = "riscv32")]
    unsafe {
        unsafe extern "C" {
            static __nv_storage_start: u8;
            static __nv_storage_length: u8;
        }

        let storage_len = &raw const __nv_storage_length as usize;
        let storage = core::slice::from_raw_parts(&raw const __nv_storage_start, storage_len);
        if let Some(data) = find_nv_value(storage, _key) {
            if !actual_len.is_null() {
                *actual_len = data.len() as u16;
            }
            if _value.is_null() || data.len() > _max_len as usize {
                trace_nv(_key, _max_len, data.len() as u16, crate::OSAL_NOK as u32);
                return crate::OSAL_NOK as u32;
            }
            core::ptr::copy_nonoverlapping(data.as_ptr(), _value, data.len());
            trace_nv(_key, _max_len, data.len() as u16, crate::OSAL_OK as u32);
            return crate::OSAL_OK as u32;
        }
    }

    trace_nv(_key, _max_len, 0, crate::OSAL_NOK as u32);
    crate::OSAL_NOK as u32
}

/// Write an item to non-volatile storage. STUB: returns failure because no
/// persistent backing has been wired yet.
#[unsafe(no_mangle)]
pub extern "C" fn uapi_nv_write(_key: u16, _value: *const u8, _len: u16) -> u32 {
    crate::OSAL_NOK as u32
}

#[cfg(any(target_arch = "riscv32", test))]
const NV_PAGE_SIZE: usize = 0x1000;
#[cfg(any(target_arch = "riscv32", test))]
const NV_PAGE_HEADER_SIZE: usize = 16;
#[cfg(any(target_arch = "riscv32", test))]
const NV_KEY_HEADER_SIZE: usize = 16;
#[cfg(any(target_arch = "riscv32", test))]
const NV_KEY_CRC_SIZE: usize = 4;
#[cfg(any(target_arch = "riscv32", test))]
const NV_STORE_ID_ACPU: u16 = 0x254d;
#[cfg(any(target_arch = "riscv32", test))]
const NV_KEY_MAGIC: u8 = 0xa9;
#[cfg(any(target_arch = "riscv32", test))]
const NV_KEY_VALID: u8 = 0xff;

#[cfg(any(target_arch = "riscv32", test))]
fn find_nv_value(storage: &[u8], wanted_key: u16) -> Option<&[u8]> {
    let page_count = storage.len() / NV_PAGE_SIZE;
    for page_index in 0..page_count {
        let start = page_index * NV_PAGE_SIZE;
        let page = &storage[start..start + NV_PAGE_SIZE];
        let details = u32::from_le_bytes(page[0..4].try_into().ok()?);
        let inverted_details = u32::from_le_bytes(page[4..8].try_into().ok()?);
        let sequence = u32::from_le_bytes(page[8..12].try_into().ok()?);
        let inverted_sequence = u32::from_le_bytes(page[12..16].try_into().ok()?);
        if details as u16 != NV_STORE_ID_ACPU
            || inverted_details != !details
            || inverted_sequence != !sequence
        {
            continue;
        }

        let mut offset = NV_PAGE_HEADER_SIZE;
        while offset + NV_KEY_HEADER_SIZE + NV_KEY_CRC_SIZE <= page.len() {
            let header = &page[offset..offset + NV_KEY_HEADER_SIZE];
            if header[0] == 0xff {
                break;
            }
            if header[0] != NV_KEY_MAGIC {
                offset += 4;
                continue;
            }

            let length = u16::from_le_bytes([header[2], header[3]]) as usize;
            let key = u16::from_le_bytes([header[6], header[7]]);
            let encrypted_key = u16::from_le_bytes([header[8], header[9]]);
            let padded_len = (length + 3) & !3;
            let record_len = NV_KEY_HEADER_SIZE + padded_len + NV_KEY_CRC_SIZE;
            if record_len < NV_KEY_HEADER_SIZE || offset + record_len > page.len() {
                break;
            }

            let crc_input_end = offset + NV_KEY_HEADER_SIZE + padded_len;
            let stored_crc = &page[crc_input_end..crc_input_end + NV_KEY_CRC_SIZE];
            let crc = crc32(&page[offset..crc_input_end]);
            let crc_matches = stored_crc == crc.to_be_bytes();
            if header[1] == NV_KEY_VALID && encrypted_key == 0 && key == wanted_key && crc_matches {
                let data_start = offset + NV_KEY_HEADER_SIZE;
                return Some(&page[data_start..data_start + length]);
            }
            offset += record_len;
        }
    }
    None
}

#[cfg(any(target_arch = "riscv32", test))]
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

// ── eFuse / TRNG / device identity ───────────────────────────────────────────
#[cfg(test)]
mod nv_tests {
    use super::{
        NV_KEY_HEADER_SIZE, NV_PAGE_HEADER_SIZE, NV_PAGE_SIZE, crc32, find_nv_value,
        tcxo_vendor_id, uapi_tsensor_get_current_temp,
    };
    use hisi_riscv_hal::clock_init::TcxoFreq;

    fn page_with_key() -> [u8; NV_PAGE_SIZE] {
        let mut page = [0xff; NV_PAGE_SIZE];
        let details = 0x0001_254d_u32;
        page[0..4].copy_from_slice(&details.to_le_bytes());
        page[4..8].copy_from_slice(&(!details).to_le_bytes());
        page[8..12].copy_from_slice(&0_u32.to_le_bytes());
        page[12..16].copy_from_slice(&u32::MAX.to_le_bytes());

        let offset = NV_PAGE_HEADER_SIZE;
        page[offset] = 0xa9;
        page[offset + 1] = 0xff;
        page[offset + 2..offset + 4].copy_from_slice(&3_u16.to_le_bytes());
        page[offset + 4] = 0xff;
        page[offset + 5] = 0xff;
        page[offset + 6..offset + 8].copy_from_slice(&0x2003_u16.to_le_bytes());
        page[offset + 8..offset + 10].copy_from_slice(&0_u16.to_le_bytes());
        page[offset + 10..offset + 12].copy_from_slice(&u16::MAX.to_le_bytes());
        page[offset + 12..offset + 16].copy_from_slice(&u32::MAX.to_le_bytes());

        let data = offset + NV_KEY_HEADER_SIZE;
        page[data..data + 4].copy_from_slice(&[1, 2, 3, 0]);
        let crc = crc32(&page[offset..data + 4]);
        page[data + 4..data + 8].copy_from_slice(&crc.to_be_bytes());
        page
    }

    #[test]
    fn finds_crc_valid_plaintext_key() {
        let page = page_with_key();
        assert_eq!(find_nv_value(&page, 0x2003), Some(&[1, 2, 3][..]));
    }

    #[test]
    fn rejects_corrupt_crc() {
        let mut page = page_with_key();
        page[NV_PAGE_HEADER_SIZE + NV_KEY_HEADER_SIZE] ^= 1;
        assert_eq!(find_nv_value(&page, 0x2003), None);
    }

    #[test]
    fn rejects_corrupt_page_header() {
        let mut page = page_with_key();
        page[4] ^= 1;
        assert_eq!(find_nv_value(&page, 0x2003), None);
    }

    #[test]
    fn tsensor_contract_writes_output_and_returns_status() {
        let mut temp = 0_i8;
        assert_eq!(uapi_tsensor_get_current_temp(&mut temp), 0);
        assert_eq!(temp, 25);
        assert_ne!(uapi_tsensor_get_current_temp(core::ptr::null_mut()), 0);
    }

    #[test]
    fn tcxo_contract_uses_vendor_enum_not_hertz() {
        assert_eq!(tcxo_vendor_id(TcxoFreq::MHz40), 0);
        assert_eq!(tcxo_vendor_id(TcxoFreq::MHz24), 1);
    }
}

// These feed RF calibration, the MAC address and crypto seeding. eFuse reads
// use the HAL while the `Wifi` handle owns its unique peripheral token. TRNG and
// device-address policy remain separate follow-up work.

/// Read one eFuse bit through the HAL-owned WS63 controller.
#[unsafe(no_mangle)]
pub extern "C" fn uapi_efuse_read_bit(value: *mut u8, byte: u32, bit: u8) -> u32 {
    if value.is_null() || bit >= 8 || !EFUSE_READY.load(Ordering::Acquire) {
        return crate::OSAL_NOK as u32;
    }
    let Some(address) = u16::try_from(byte)
        .ok()
        .and_then(hisi_riscv_hal::efuse::EfuseByteAddress::from_byte)
    else {
        return crate::OSAL_NOK as u32;
    };
    // SAFETY: `Wifi` keeps the unique eFuse token alive after enabling reads;
    // the HAL serializes the complete read transaction.
    let byte = unsafe { hisi_riscv_hal::efuse::EfuseDriver::read_byte_unchecked(address) };
    // SAFETY: the SDK ABI defines `value` as a writable one-byte output.
    unsafe { value.write((byte >> bit) & 1) };
    crate::OSAL_OK as u32
}

/// Read consecutive eFuse bytes through the HAL-owned WS63 controller.
#[unsafe(no_mangle)]
pub extern "C" fn uapi_efuse_read_buffer(buffer: *mut u8, byte: u32, length: u16) -> u32 {
    if (buffer.is_null() && length != 0) || !EFUSE_READY.load(Ordering::Acquire) {
        return crate::OSAL_NOK as u32;
    }
    let Some(start) = u16::try_from(byte).ok() else {
        return crate::OSAL_NOK as u32;
    };
    for offset in 0..length {
        let Some(address) = start
            .checked_add(offset)
            .and_then(hisi_riscv_hal::efuse::EfuseByteAddress::from_byte)
        else {
            return crate::OSAL_NOK as u32;
        };
        // SAFETY: `Wifi` holds the unique eFuse token and HAL serializes reads.
        let value = unsafe { hisi_riscv_hal::efuse::EfuseDriver::read_byte_unchecked(address) };
        // SAFETY: the SDK ABI guarantees a writable `length`-byte buffer.
        unsafe { buffer.add(offset as usize).write(value) };
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

const CLK40M_TCXO: u32 = 0;
const CLK24M_TCXO: u32 = 1;

const fn tcxo_vendor_id(freq: hisi_riscv_hal::clock_init::TcxoFreq) -> u32 {
    match freq {
        hisi_riscv_hal::clock_init::TcxoFreq::MHz40 => CLK40M_TCXO,
        hisi_riscv_hal::clock_init::TcxoFreq::MHz24 => CLK24M_TCXO,
    }
}

/// Return the SDK's TCXO selector (`0` = 40 MHz, `1` = 24 MHz).
///
/// This ABI deliberately does not return Hertz. The ROM/blob code compares the
/// result with `CLK40M_TCXO`/`CLK24M_TCXO`; returning `24_000_000` would select
/// neither valid clock path. The hardware strap is decoded by the HAL so this
/// adapter remains a conversion at the vendor boundary, not a second raw-MMIO
/// implementation.
#[unsafe(no_mangle)]
pub extern "C" fn get_tcxo_freq() -> u32 {
    #[cfg(target_arch = "riscv32")]
    let freq = hisi_riscv_hal::clock_init::TcxoFreq::detect();
    #[cfg(not(target_arch = "riscv32"))]
    let freq = hisi_riscv_hal::clock_init::TcxoFreq::MHz40;

    tcxo_vendor_id(freq)
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
