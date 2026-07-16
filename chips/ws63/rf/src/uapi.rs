//! UAPI platform services (ws63-RF `port_uapi.h`).
//!
//! Timekeeping delegates to the WS63 mask-ROM systick/TCXO drivers using the
//! vendor platform ROM-data initializer linked at its fixed DTCM ABI.
//! `uapi_nv_read` is backed by the official WS63 ACPU KV partition and validates
//! its page/key metadata and CRC. `uapi_tsensor_get_current_temp` remains a fixed
//! conservative value until the HAL sensor path is wired into the RF adapter.

// C-ABI entry points: the blob passes valid pointers; the safety contract is
// the C signature, not a Rust `unsafe` marker.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::c_void;
#[cfg(target_arch = "riscv32")]
use hisi_nvs::{NvConfig, NvError, NvKey, NvReader};
#[cfg(target_arch = "riscv32")]
use hisi_storage::MemoryMappedStorage;
use portable_atomic::{AtomicBool, Ordering};

static EFUSE_READY: AtomicBool = AtomicBool::new(false);

#[cfg(target_arch = "riscv32")]
pub(crate) fn enable_efuse_reads() {
    EFUSE_READY.store(true, Ordering::Release);
}

fn trace_nv(key: u16, max_len: u16, actual_len: u16, result: u32) {
    #[cfg(feature = "rf-init-diag")]
    crate::rf_init_diag::trace_nv(key, max_len, actual_len, result);
    #[cfg(not(feature = "rf-init-diag"))]
    let _ = (key, max_len, actual_len, result);
}

#[cfg(target_arch = "riscv32")]
unsafe extern "C" {
    #[link_name = "uapi_systick_get_ms"]
    fn rom_systick_get_ms() -> u64;
    #[link_name = "uapi_systick_get_count"]
    fn rom_systick_get_count() -> u64;
    #[link_name = "uapi_tcxo_get_us"]
    fn rom_tcxo_get_us() -> u64;
    #[link_name = "uapi_tcxo_delay_us"]
    fn rom_tcxo_delay_us(usec: u32) -> u32;
    fn uapi_systick_init();
    fn uapi_tcxo_init() -> u32;
    static mut g_systick_clock: u32;
}

#[cfg(target_arch = "riscv32")]
unsafe fn calibrate_systick_clock() {
    const CALIBRATION_US: u32 = 100_000;

    let systick_start = unsafe { rom_systick_get_count() };
    let tcxo_start = unsafe { rom_tcxo_get_us() };
    let _ = unsafe { rom_tcxo_delay_us(CALIBRATION_US) };
    let systick_delta = unsafe { rom_systick_get_count() }.wrapping_sub(systick_start);
    let tcxo_delta = unsafe { rom_tcxo_get_us() }.wrapping_sub(tcxo_start);
    if tcxo_delta == 0 {
        return;
    }

    let calibrated = systick_delta
        .saturating_mul(1_000_000)
        .saturating_add(tcxo_delta / 2)
        / tcxo_delta;
    if let Ok(clock) = u32::try_from(calibrated)
        && (1_000..=100_000).contains(&clock)
    {
        // SAFETY: this is the vendor ROM-data conversion word used by
        // `uapi_systick_get_ms`. The official LiteOS startup performs the same
        // RTC-vs-TCXO calibration before normal application work.
        unsafe { (&raw mut g_systick_clock).write_volatile(clock) };
    }
}

#[cfg(target_arch = "riscv32")]
pub(crate) fn initialize_rom_timebases() -> u32 {
    unsafe {
        // SAFETY: these are the same mask-ROM initialization calls used by the
        // vendor `hw_init`. hisi-riscv-rt has already copied the original
        // platform ROM-data initializer, including both HAL function tables and
        // the 32 kHz / 24 MHz conversion values, to their fixed DTCM ABI slots.
        uapi_systick_init();
        let result = uapi_tcxo_init();
        calibrate_systick_clock();
        result
    }
}

/// Monotonic milliseconds from the mask-ROM 32 kHz systick implementation.
///
/// This hidden callback is exported only so the application can inject the
/// chip time source into `hisi-rtos`; it is not a general RF control API. The
/// runtime must not call it before `Wifi::initialize` initializes ROM timebases.
#[doc(hidden)]
pub fn monotonic_ms() -> u64 {
    #[cfg(target_arch = "riscv32")]
    unsafe {
        // SAFETY: initialized once by `Wifi::initialize`; the ROM function only
        // reads its registered WS63 systick controller.
        rom_systick_get_ms()
    }
    #[cfg(not(target_arch = "riscv32"))]
    0
}

/// Monotonic microseconds from the mask-ROM TCXO implementation.
pub(crate) fn monotonic_us() -> u64 {
    #[cfg(target_arch = "riscv32")]
    unsafe {
        // SAFETY: same initialized ROM timebase contract as `monotonic_ms`.
        rom_tcxo_get_us()
    }
    #[cfg(not(target_arch = "riscv32"))]
    0
}

pub(crate) fn delay_us(usec: u32) {
    #[cfg(target_arch = "riscv32")]
    unsafe {
        // SAFETY: same initialized ROM TCXO contract as `monotonic_us`.
        let _ = rom_tcxo_delay_us(usec);
    }
    #[cfg(not(target_arch = "riscv32"))]
    let _ = usec;
}

/// Current chip temperature in °C.
///
/// SCAFFOLD: writes a conservative 25 °C. The pointer/result ABI matches the
/// vendor SDK; a real reading still needs the hisi-hal tsensor (RF2/RF3).
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
    key: u16,
    max_len: u16,
    actual_len: *mut u16,
    value: *mut u8,
) -> u32 {
    #[cfg(not(target_arch = "riscv32"))]
    let _ = value;
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
        // SAFETY: the linker-provided region is the flashboot-initialized,
        // read-only WS63 NV partition and remains mapped for the firmware life.
        let storage =
            MemoryMappedStorage::from_raw_parts(&raw const __nv_storage_start, storage_len);
        let Ok(mut reader) = NvReader::try_new(storage, NvConfig::WS63_ACPU) else {
            trace_nv(key, max_len, 0, crate::OSAL_NOK as u32);
            return crate::OSAL_NOK as u32;
        };
        let output = if value.is_null() {
            &mut []
        } else {
            core::slice::from_raw_parts_mut(value, max_len as usize)
        };
        match reader.read(NvKey::from_raw(key), output) {
            Ok(length) => {
                let length = length as u16;
                if !actual_len.is_null() {
                    *actual_len = length;
                }
                trace_nv(key, max_len, length, crate::OSAL_OK as u32);
                return crate::OSAL_OK as u32;
            }
            Err(NvError::BufferTooSmall { required }) => {
                let required = u16::try_from(required).unwrap_or(u16::MAX);
                if !actual_len.is_null() {
                    *actual_len = required;
                }
                trace_nv(key, max_len, required, crate::OSAL_NOK as u32);
                return crate::OSAL_NOK as u32;
            }
            Err(_) => {}
        }
    }

    trace_nv(key, max_len, 0, crate::OSAL_NOK as u32);
    crate::OSAL_NOK as u32
}

/// Write an item to non-volatile storage. STUB: returns failure because no
/// persistent backing has been wired yet.
#[unsafe(no_mangle)]
pub extern "C" fn uapi_nv_write(_key: u16, _value: *const u8, _len: u16) -> u32 {
    crate::OSAL_NOK as u32
}

// ── eFuse / TRNG / device identity ───────────────────────────────────────────
#[cfg(test)]
mod uapi_tests {
    use super::{tcxo_vendor_id, uapi_tsensor_get_current_temp};
    use hisi_hal::clock_init::TcxoFreq;

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
        .and_then(hisi_hal::efuse::EfuseByteAddress::from_byte)
    else {
        return crate::OSAL_NOK as u32;
    };
    // SAFETY: `Wifi` keeps the unique eFuse token alive after enabling reads;
    // the HAL serializes the complete read transaction.
    let byte = unsafe { hisi_hal::efuse::EfuseDriver::read_byte_unchecked(address) };
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
            .and_then(hisi_hal::efuse::EfuseByteAddress::from_byte)
        else {
            return crate::OSAL_NOK as u32;
        };
        // SAFETY: `Wifi` holds the unique eFuse token and HAL serializes reads.
        let value = unsafe { hisi_hal::efuse::EfuseDriver::read_byte_unchecked(address) };
        // SAFETY: the SDK ABI guarantees a writable `length`-byte buffer.
        unsafe { buffer.add(offset as usize).write(value) };
    }
    crate::OSAL_OK as u32
}

/// Fill random bytes through the uniquely owned WS63 hardware TRNG.
#[unsafe(no_mangle)]
pub extern "C" fn uapi_drv_cipher_trng_get_random_bytes(randnum: *mut u8, size: u32) -> u32 {
    if randnum.is_null() && size != 0 {
        return crate::OSAL_NOK as u32;
    }
    if size == 0 {
        return crate::OSAL_OK as u32;
    }
    // SAFETY: null was rejected and the C ABI promises `size` writable bytes.
    let output = unsafe { core::slice::from_raw_parts_mut(randnum, size as usize) };
    crate::crypto::fill_hardware_entropy(output)
        .map(|()| crate::OSAL_OK as u32)
        .unwrap_or(crate::OSAL_NOK as u32)
}

const NV_ID_SYSTEM_FACTORY_MAC: u16 = 0x0005;
static mut WIFI_BASE_MAC: [u8; 6] = [0; 6];
static WIFI_BASE_MAC_READY: portable_atomic::AtomicBool = portable_atomic::AtomicBool::new(false);

fn valid_unicast_mac(mac: &[u8; 6]) -> bool {
    mac[0] & 1 == 0 && *mac != [0; 6] && *mac != [0xff; 6]
}

fn wifi_base_mac() -> [u8; 6] {
    critical_section::with(|_| {
        if !WIFI_BASE_MAC_READY.load(Ordering::Relaxed) {
            let mut mac = [0; 6];
            let mut actual = 0_u16;
            if uapi_nv_read(
                NV_ID_SYSTEM_FACTORY_MAC,
                mac.len() as u16,
                &mut actual,
                mac.as_mut_ptr(),
            ) != crate::OSAL_OK as u32
                || actual != mac.len() as u16
                || !valid_unicast_mac(&mac)
            {
                let mut found = false;
                // SDK efuse items 12..9: 48-bit MAC slots at bit
                // 1728, 1680, 1632 and 1584, newest slot first.
                for byte_offset in [216_u32, 210, 204, 198] {
                    if uapi_efuse_read_buffer(mac.as_mut_ptr(), byte_offset, mac.len() as u16)
                        == crate::OSAL_OK as u32
                        && valid_unicast_mac(&mac)
                    {
                        found = true;
                        break;
                    }
                }
                if !found {
                    let _ =
                        uapi_drv_cipher_trng_get_random_bytes(mac.as_mut_ptr(), mac.len() as u32);
                    mac[0] = (mac[0] & 0xfc) | 0x02;
                    mac[1] = 0x00;
                    mac[2] = 0x73;
                }
            }
            // SAFETY: all accesses are serialized by the single-hart critical
            // section and readiness is published only after the full copy.
            unsafe { core::ptr::write(&raw mut WIFI_BASE_MAC, mac) };
            WIFI_BASE_MAC_READY.store(true, Ordering::Relaxed);
        }
        // SAFETY: initialized before READY and read under the same lock.
        unsafe { core::ptr::read(&raw const WIFI_BASE_MAC) }
    })
}

/// Device address following the WS63 SDK's base-MAC and interface derivation
/// rules. The base Wi-Fi MAC comes from factory KV key `0x0005`; an ephemeral
/// locally-administered unicast address is used only when factory data is
/// unavailable or invalid.
#[unsafe(no_mangle)]
pub extern "C" fn get_dev_addr(pc_addr: *mut u8, addr_len: u8, interface_type: u8) -> u32 {
    if pc_addr.is_null() || addr_len != 6 {
        return crate::OSAL_NOK as u32;
    }
    let mut mac = wifi_base_mac();
    let derive = match interface_type {
        2 => 0_u16,      // station
        3 => 2_u16,      // AP
        7..=10 => 3_u16, // mesh / P2P
        _ => return crate::OSAL_NOK as u32,
    };
    let mut carry = derive;
    for byte in mac.iter_mut().rev() {
        carry += *byte as u16;
        *byte = carry as u8;
        carry >>= 8;
    }
    mac[0] &= 0xfe;
    // SAFETY: caller guarantees `addr_len` bytes.
    unsafe { core::ptr::copy_nonoverlapping(mac.as_ptr(), pc_addr, mac.len()) };
    crate::OSAL_OK as u32
}

const CLK40M_TCXO: u32 = 0;
const CLK24M_TCXO: u32 = 1;

const fn tcxo_vendor_id(freq: hisi_hal::clock_init::TcxoFreq) -> u32 {
    match freq {
        hisi_hal::clock_init::TcxoFreq::MHz40 => CLK40M_TCXO,
        hisi_hal::clock_init::TcxoFreq::MHz24 => CLK24M_TCXO,
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
    let freq = hisi_hal::clock_init::TcxoFreq::detect();
    #[cfg(not(target_arch = "riscv32"))]
    let freq = hisi_hal::clock_init::TcxoFreq::MHz40;

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
#[cfg(not(feature = "wifi-personal"))]
#[unsafe(no_mangle)]
pub extern "C" fn uapi_wifi_softap_stop() -> i32 {
    crate::OSAL_OK
}

/// Stop the station. STUB.
#[cfg(not(feature = "wifi-personal"))]
#[unsafe(no_mangle)]
pub extern "C" fn uapi_wifi_sta_stop() -> i32 {
    crate::OSAL_OK
}
