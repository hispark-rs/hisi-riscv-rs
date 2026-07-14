//! Native runtime hooks for the pinned upstream hostap port.
//!
//! The C port owns no scheduler and does not emulate LiteOS. It delegates only
//! the capabilities needed by the single `RadioRunner` through
//! `hisi-rf-rtos-driver`, the RF heap, the WS63 monotonic clock and the explicit
//! WS63 entropy backend.

use core::cell::UnsafeCell;
use core::ffi::{c_int, c_void};
use core::num::NonZeroU32;

use hisi_rf_rtos_driver::{Semaphore, WaitOutcome, WaitTimeout};
use portable_atomic::{AtomicU8, Ordering};
use ws63_radio_sys::supplicant::{
    ABI_VERSION, DriverHooks, Key, OsHooks, cipher, hisi_wpa_driver_install, hisi_wpa_os_install,
    hisi_wpa_os_uninstall, key_flag,
};

static RUNNER_WAKE: Semaphore = Semaphore::new(0);
static PORT_IDENTITY: u8 = 0;
static DRIVER_CONTEXT: DriverContext = DriverContext::new();
static PORT_STATE: AtomicU8 = AtomicU8::new(PORT_FREE);

const PORT_FREE: u8 = 0;
const PORT_INSTALLING: u8 = 1;
const PORT_READY: u8 = 2;
const PORT_POISONED: u8 = 3;

const ETHERNET_HEADER_LEN: usize = 14;
const EAPOL_ETHERTYPE: [u8; 2] = [0x88, 0x8e];
// The Personal-only vendor profile uses EAPOL_PKT_BUF_SIZE=800. Enterprise is
// a separate future profile and must raise this bound explicitly.
const MAX_EAPOL_PAYLOAD_LEN: usize = 800;
const IFNAME_CAPACITY: usize = 17;

const IOCTL_NEW_KEY: u32 = 1;
const IOCTL_DEL_KEY: u32 = 2;
const IOCTL_SET_KEY: u32 = 3;
const IOCTL_SEND_MLME: u32 = 4;
const IOCTL_SEND_EAPOL: u32 = 5;
const IOCTL_GET_ADDRESS: u32 = 9;

const KEY_TYPE_GROUP: i32 = 0;
const KEY_TYPE_PAIRWISE: i32 = 1;
const KEY_DEFAULT_INVALID: u8 = 0;
const KEY_DEFAULT_UNICAST: u8 = 1;
const KEY_DEFAULT_MULTICAST: u8 = 2;

struct DriverContext {
    ifname: UnsafeCell<[u8; IFNAME_CAPACITY]>,
    send_action_cookie: UnsafeCell<u64>,
}

// SAFETY: the interface name is written only while PORT_INSTALLING and is
// published by PORT_READY. Hostap serializes driver control calls through its
// single RadioRunner; only the management callback mutates the cookie.
unsafe impl Sync for DriverContext {}

impl DriverContext {
    const fn new() -> Self {
        Self {
            ifname: UnsafeCell::new([0; IFNAME_CAPACITY]),
            send_action_cookie: UnsafeCell::new(0),
        }
    }

    fn initialize(&self, ifname: &[u8]) -> bool {
        if ifname.is_empty() || ifname.len() >= IFNAME_CAPACITY || ifname.contains(&0) {
            return false;
        }
        let mut stored = [0; IFNAME_CAPACITY];
        stored[..ifname.len()].copy_from_slice(ifname);
        // SAFETY: the caller owns the PORT_INSTALLING state; no callback can
        // observe the context before driver registration and PORT_READY.
        unsafe {
            self.ifname.get().write(stored);
            self.send_action_cookie.get().write(0);
        }
        true
    }

    fn matches(&self, ifname: &[u8]) -> bool {
        // SAFETY: PORT_READY publishes the initialized immutable name.
        let stored = unsafe { &*self.ifname.get() };
        stored
            .iter()
            .position(|byte| *byte == 0)
            .is_some_and(|len| &stored[..len] == ifname)
    }

    fn ifname(&self) -> &[u8; IFNAME_CAPACITY] {
        // SAFETY: callbacks are installed only after initialization and the
        // name remains immutable for the firmware lifetime.
        unsafe { &*self.ifname.get() }
    }
}

#[repr(C)]
struct TxEapol {
    buffer: *mut u8,
    length: u32,
}

#[repr(C)]
struct MlmeData {
    frequency_mhz: u32,
    data_len: u32,
    data: *mut u8,
    send_action_cookie: *mut u64,
}

#[repr(C)]
struct KeyExtension {
    key_type: i32,
    key_index: u32,
    key_len: u32,
    sequence_len: u32,
    cipher: u32,
    address: *mut u8,
    material: *mut u8,
    sequence: *mut u8,
    default_data: u8,
    default_management: u8,
    default_types: u8,
    reserved: u8,
}

#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(core::mem::size_of::<TxEapol>() == 8);
    assert!(core::mem::size_of::<MlmeData>() == 16);
    assert!(core::mem::size_of::<KeyExtension>() == 36);
    assert!(core::mem::offset_of!(KeyExtension, address) == 20);
    assert!(core::mem::offset_of!(KeyExtension, default_data) == 32);
};

/// Failure while registering the upstream supplicant native runtime seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpstreamSupplicantPortError {
    /// The application has not installed a runtime or exhausted its resources.
    Runtime(hisi_rf_rtos_driver::Error),
    /// The interface name is empty, contains NUL, or exceeds the WS63 ABI.
    InvalidInterfaceName,
    /// Another context is currently installing the singleton port.
    Busy,
    /// A previous failed rollback left the singleton registration uncertain.
    Poisoned,
    /// The singleton is already installed for another interface.
    InterfaceConflict,
    /// The C ABI rejected the hook table or another runtime already owns it.
    Abi(i32),
    /// Driver installation failed and the OS hook rollback also failed.
    Rollback { install: i32, rollback: i32 },
}

/// Install the native OS/eloop hooks after `hisi-rtos` has installed its RF
/// runtime contract and after the WS63 ROM timebases are initialized.
///
/// The C boundary copies the hook table synchronously, so no Rust reference is
/// retained. Calling this repeatedly with the same hooks is idempotent.
pub fn prepare_upstream_supplicant_port(ifname: &[u8]) -> Result<(), UpstreamSupplicantPortError> {
    RUNNER_WAKE
        .try_init()
        .map_err(UpstreamSupplicantPortError::Runtime)?;

    match PORT_STATE.load(Ordering::Acquire) {
        PORT_READY => {
            return DRIVER_CONTEXT
                .matches(ifname)
                .then_some(())
                .ok_or(UpstreamSupplicantPortError::InterfaceConflict);
        }
        PORT_INSTALLING => return Err(UpstreamSupplicantPortError::Busy),
        PORT_POISONED => return Err(UpstreamSupplicantPortError::Poisoned),
        PORT_FREE => {}
        _ => return Err(UpstreamSupplicantPortError::Poisoned),
    }
    PORT_STATE
        .compare_exchange(
            PORT_FREE,
            PORT_INSTALLING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .map_err(|_| UpstreamSupplicantPortError::Busy)?;
    if !DRIVER_CONTEXT.initialize(ifname) {
        PORT_STATE.store(PORT_FREE, Ordering::Release);
        return Err(UpstreamSupplicantPortError::InvalidInterfaceName);
    }

    let os_hooks = OsHooks {
        abi_version: ABI_VERSION,
        reserved: 0,
        context: core::ptr::addr_of!(PORT_IDENTITY).cast_mut().cast(),
        allocate_zeroed: Some(allocate_zeroed),
        reallocate_zeroed: Some(reallocate_zeroed),
        deallocate: Some(deallocate),
        monotonic_us: Some(monotonic_us),
        wall_clock_us: None,
        sleep_ms: Some(sleep_ms),
        fill_entropy: Some(fill_entropy),
        wait_for_work: Some(wait_for_work),
        wake_runner: Some(wake_runner),
    };
    let driver_hooks = DriverHooks {
        abi_version: ABI_VERSION,
        reserved: 0,
        driver: core::ptr::addr_of!(DRIVER_CONTEXT).cast_mut().cast(),
        get_own_address: Some(get_own_address),
        send_eapol: Some(send_eapol),
        send_mgmt: Some(send_mgmt),
        install_key: Some(install_key),
        remove_key: Some(remove_key),
    };
    // SAFETY: `os_hooks` matches ABI_VERSION and the C function copies the table
    // before returning. Every callback has C ABI and static backing state.
    let os_result = unsafe { hisi_wpa_os_install(&raw const os_hooks) };
    if os_result != 0 {
        PORT_STATE.store(PORT_FREE, Ordering::Release);
        return Err(UpstreamSupplicantPortError::Abi(os_result));
    }
    // SAFETY: the versioned table contains only C ABI callbacks with static
    // backing state and is copied by the C boundary before returning.
    let driver_result = unsafe { hisi_wpa_driver_install(&raw const driver_hooks) };
    if driver_result == 0 {
        PORT_STATE.store(PORT_READY, Ordering::Release);
        return Ok(());
    }

    // No driver/L2 user can exist before driver registration succeeds, so the
    // OS registration made above can be rolled back synchronously.
    let rollback = unsafe { hisi_wpa_os_uninstall(os_hooks.context) };
    if rollback == 0 {
        PORT_STATE.store(PORT_FREE, Ordering::Release);
        Err(UpstreamSupplicantPortError::Abi(driver_result))
    } else {
        PORT_STATE.store(PORT_POISONED, Ordering::Release);
        Err(UpstreamSupplicantPortError::Rollback {
            install: driver_result,
            rollback,
        })
    }
}

fn build_eapol_frame<'a>(
    destination: &[u8; 6],
    source: &[u8; 6],
    payload: &[u8],
    storage: &'a mut [u8; ETHERNET_HEADER_LEN + MAX_EAPOL_PAYLOAD_LEN],
) -> Option<&'a [u8]> {
    if payload.is_empty() || payload.len() > MAX_EAPOL_PAYLOAD_LEN {
        return None;
    }
    storage[..6].copy_from_slice(destination);
    storage[6..12].copy_from_slice(source);
    storage[12..ETHERNET_HEADER_LEN].copy_from_slice(&EAPOL_ETHERTYPE);
    storage[ETHERNET_HEADER_LEN..ETHERNET_HEADER_LEN + payload.len()].copy_from_slice(payload);
    Some(&storage[..ETHERNET_HEADER_LEN + payload.len()])
}

fn driver_context(driver: *mut c_void) -> Option<&'static DriverContext> {
    (driver == core::ptr::addr_of!(DRIVER_CONTEXT).cast_mut().cast()).then_some(&DRIVER_CONTEXT)
}

unsafe extern "C" fn get_own_address(driver: *mut c_void, address: *mut u8) -> c_int {
    let Some(driver) = driver_context(driver) else {
        return -1;
    };
    if address.is_null() {
        return -1;
    }
    if let Some(live) = crate::netif::hardware_address() {
        // SAFETY: the callback contract provides six writable bytes.
        unsafe { core::ptr::copy_nonoverlapping(live.as_ptr(), address, live.len()) };
        0
    } else {
        crate::wal::ioctl(driver.ifname(), IOCTL_GET_ADDRESS, address.cast())
    }
}

unsafe extern "C" fn send_eapol(
    driver: *mut c_void,
    destination: *const u8,
    payload: *const u8,
    payload_len: usize,
) -> c_int {
    let Some(driver) = driver_context(driver) else {
        return -1;
    };
    if destination.is_null()
        || payload.is_null()
        || payload_len == 0
        || payload_len > MAX_EAPOL_PAYLOAD_LEN
    {
        return -1;
    }
    let mut source = [0; 6];
    if unsafe {
        get_own_address(
            core::ptr::from_ref(driver).cast_mut().cast(),
            source.as_mut_ptr(),
        )
    } != 0
    {
        return -1;
    }
    // SAFETY: the callback contract supplies six destination bytes and
    // `payload_len` readable payload bytes for this synchronous call.
    let destination = unsafe { &*destination.cast::<[u8; 6]>() };
    let payload = unsafe { core::slice::from_raw_parts(payload, payload_len) };
    let mut storage = [0; ETHERNET_HEADER_LEN + MAX_EAPOL_PAYLOAD_LEN];
    let Some(frame) = build_eapol_frame(destination, &source, payload, &mut storage) else {
        return -1;
    };
    let mut request = TxEapol {
        buffer: frame.as_ptr().cast_mut(),
        length: frame.len() as u32,
    };
    crate::wal::ioctl(
        driver.ifname(),
        IOCTL_SEND_EAPOL,
        (&mut request as *mut TxEapol).cast(),
    )
}

unsafe extern "C" fn send_mgmt(
    driver: *mut c_void,
    frequency_mhz: u32,
    frame: *const u8,
    frame_len: usize,
) -> c_int {
    let Some(driver) = driver_context(driver) else {
        return -1;
    };
    if frame.is_null() || frame_len == 0 || frame_len > u32::MAX as usize {
        return -1;
    }
    let mut request = MlmeData {
        frequency_mhz,
        data_len: frame_len as u32,
        data: frame.cast_mut(),
        send_action_cookie: driver.send_action_cookie.get(),
    };
    crate::wal::ioctl(
        driver.ifname(),
        IOCTL_SEND_MLME,
        (&mut request as *mut MlmeData).cast(),
    )
}

unsafe extern "C" fn install_key(
    driver: *mut c_void,
    key: *const Key,
    material: *const u8,
    material_len: usize,
) -> c_int {
    let Some(driver) = driver_context(driver) else {
        return -1;
    };
    let Some(key) = (unsafe { key.as_ref() }) else {
        return -1;
    };
    if material.is_null() || !valid_key_material(key.cipher, material_len) {
        return -1;
    }
    let Some(mut request) = key_request(key, material.cast_mut(), material_len) else {
        return -1;
    };
    let install = crate::wal::ioctl(
        driver.ifname(),
        IOCTL_NEW_KEY,
        (&mut request as *mut KeyExtension).cast(),
    );
    if install != 0 || key.flags & key_flag::TX == 0 {
        return install;
    }
    let set_default = crate::wal::ioctl(
        driver.ifname(),
        IOCTL_SET_KEY,
        (&mut request as *mut KeyExtension).cast(),
    );
    if set_default != 0 {
        let _ = crate::wal::ioctl(
            driver.ifname(),
            IOCTL_DEL_KEY,
            (&mut request as *mut KeyExtension).cast(),
        );
    }
    set_default
}

unsafe extern "C" fn remove_key(driver: *mut c_void, key: *const Key) -> c_int {
    let Some(driver) = driver_context(driver) else {
        return -1;
    };
    let Some(key) = (unsafe { key.as_ref() }) else {
        return -1;
    };
    let Some(mut request) = key_request(key, core::ptr::null_mut(), 0) else {
        return -1;
    };
    crate::wal::ioctl(
        driver.ifname(),
        IOCTL_DEL_KEY,
        (&mut request as *mut KeyExtension).cast(),
    )
}

fn key_request(key: &Key, material: *mut u8, material_len: usize) -> Option<KeyExtension> {
    const ALLOWED_FLAGS: u32 =
        key_flag::DEFAULT | key_flag::RX | key_flag::TX | key_flag::GROUP | key_flag::PAIRWISE;
    if key.abi_version != ABI_VERSION
        || key.sequence_len as usize > key.sequence.len()
        || key.flags & !ALLOWED_FLAGS != 0
        || key.flags & (key_flag::MODIFY | key_flag::PMK) != 0
        || key.flags & (key_flag::RX | key_flag::TX) == 0
    {
        return None;
    }
    let pairwise = key.flags & key_flag::PAIRWISE != 0;
    let group = key.flags & key_flag::GROUP != 0;
    if pairwise == group {
        return None;
    }
    let broadcast = key.peer == [0xff; 6];
    if key.peer_present > 1
        || (pairwise && (key.peer_present != 1 || broadcast))
        || (group && key.peer_present == 1 && !broadcast)
    {
        return None;
    }
    let address = if pairwise {
        key.peer.as_ptr().cast_mut()
    } else {
        core::ptr::null_mut()
    };
    let management = matches!(
        key.cipher,
        cipher::BIP_CMAC_128 | cipher::BIP_GMAC_128 | cipher::BIP_GMAC_256 | cipher::BIP_CMAC_256
    );
    Some(KeyExtension {
        key_type: if pairwise {
            KEY_TYPE_PAIRWISE
        } else {
            KEY_TYPE_GROUP
        },
        key_index: key.key_index as u32,
        key_len: material_len as u32,
        sequence_len: key.sequence_len as u32,
        cipher: cipher_suite(key.cipher, material_len)?,
        address,
        material,
        sequence: if key.sequence_len != 0 {
            key.sequence.as_ptr().cast_mut()
        } else {
            core::ptr::null_mut()
        },
        default_data: (!management) as u8,
        default_management: management as u8,
        default_types: if pairwise {
            KEY_DEFAULT_UNICAST
        } else if key.peer_present == 1 {
            KEY_DEFAULT_MULTICAST
        } else {
            KEY_DEFAULT_INVALID
        },
        reserved: 0,
    })
}

fn valid_key_material(cipher: u8, len: usize) -> bool {
    match cipher {
        cipher::WEP => matches!(len, 5 | 13),
        cipher::TKIP
        | cipher::GCMP_256
        | cipher::CCMP_256
        | cipher::BIP_GMAC_256
        | cipher::BIP_CMAC_256 => len == 32,
        cipher::CCMP | cipher::BIP_CMAC_128 | cipher::GCMP | cipher::BIP_GMAC_128 => len == 16,
        _ => false,
    }
}

fn cipher_suite(cipher: u8, material_len: usize) -> Option<u32> {
    let selector = match cipher {
        cipher::NONE => 0,
        cipher::WEP if material_len == 5 => 1,
        cipher::WEP if material_len == 13 => 5,
        cipher::TKIP => 2,
        cipher::CCMP => 4,
        cipher::BIP_CMAC_128 => 6,
        cipher::GCMP => 8,
        cipher::GCMP_256 => 9,
        cipher::CCMP_256 => 10,
        cipher::BIP_GMAC_128 => 11,
        cipher::BIP_GMAC_256 => 12,
        cipher::BIP_CMAC_256 => 13,
        _ => return None,
    };
    Some(if selector == 0 {
        0
    } else {
        0x000f_ac00 | selector
    })
}

unsafe extern "C" fn allocate_zeroed(_: *mut c_void, size: usize, alignment: usize) -> *mut c_void {
    crate::alloc::allocate_zeroed(size, alignment)
}

unsafe extern "C" fn reallocate_zeroed(
    _: *mut c_void,
    pointer: *mut c_void,
    size: usize,
    alignment: usize,
) -> *mut c_void {
    // SAFETY: the C port passes only null or pointers obtained through the
    // matching allocation callback. CHeap validates ownership defensively.
    unsafe { crate::alloc::reallocate_zeroed(pointer, size, alignment) }
}

unsafe extern "C" fn deallocate(_: *mut c_void, pointer: *mut c_void) {
    crate::alloc::osal_kfree(pointer);
}

unsafe extern "C" fn monotonic_us(_: *mut c_void, value: *mut u64) -> c_int {
    let Some(value) = (unsafe { value.as_mut() }) else {
        return -1;
    };
    *value = crate::uapi::monotonic_us();
    0
}

unsafe extern "C" fn sleep_ms(_: *mut c_void, milliseconds: u32) -> c_int {
    let result = if let Some(milliseconds) = NonZeroU32::new(milliseconds) {
        hisi_rf_rtos_driver::sleep_ms(milliseconds)
    } else {
        hisi_rf_rtos_driver::yield_now()
    };
    result.map(|()| 0).unwrap_or(-1)
}

unsafe extern "C" fn fill_entropy(_: *mut c_void, output: *mut u8, output_len: usize) -> c_int {
    if output_len == 0 {
        return 0;
    }
    if output.is_null() {
        return -1;
    }
    #[cfg(target_arch = "riscv32")]
    {
        use hisi_crypto::EntropySource;

        // SAFETY: null was rejected and the C ABI promises `output_len`
        // writable bytes for the duration of this call.
        let output = unsafe { core::slice::from_raw_parts_mut(output, output_len) };
        crate::crypto::WS63_CRYPTO
            .fill_entropy(output)
            .map(|()| 0)
            .unwrap_or(-1)
    }
    #[cfg(not(target_arch = "riscv32"))]
    {
        let _ = output;
        -1
    }
}

unsafe extern "C" fn wait_for_work(_: *mut c_void, timeout_ms: u32) -> c_int {
    match RUNNER_WAKE.down_timeout(WaitTimeout::from_millis(timeout_ms)) {
        Ok(WaitOutcome::Acquired) | Ok(WaitOutcome::TimedOut) => 0,
        Err(_) => -1,
    }
}

unsafe extern "C" fn wake_runner(_: *mut c_void) {
    let _ = RUNNER_WAKE.up();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_registration_before_runtime_installation() {
        assert_eq!(
            prepare_upstream_supplicant_port(b"wlan0"),
            Err(UpstreamSupplicantPortError::Runtime(
                hisi_rf_rtos_driver::Error::NotInstalled
            ))
        );
    }

    #[test]
    fn builds_bounded_ethernet_eapol_frame() {
        let destination = [1, 2, 3, 4, 5, 6];
        let source = [6, 5, 4, 3, 2, 1];
        let payload = [2, 3, 0, 5];
        let mut storage = [0; ETHERNET_HEADER_LEN + MAX_EAPOL_PAYLOAD_LEN];
        let frame = build_eapol_frame(&destination, &source, &payload, &mut storage).unwrap();
        assert_eq!(&frame[..6], &destination);
        assert_eq!(&frame[6..12], &source);
        assert_eq!(&frame[12..14], &EAPOL_ETHERTYPE);
        assert_eq!(&frame[14..], &payload);
    }

    #[test]
    fn rejects_empty_or_oversized_eapol_payload() {
        let mut storage = [0; ETHERNET_HEADER_LEN + MAX_EAPOL_PAYLOAD_LEN];
        assert!(build_eapol_frame(&[0; 6], &[0; 6], &[], &mut storage).is_none());
        assert!(
            build_eapol_frame(
                &[0; 6],
                &[0; 6],
                &[0; MAX_EAPOL_PAYLOAD_LEN + 1],
                &mut storage,
            )
            .is_none()
        );
    }

    #[test]
    fn validates_key_material_and_vendor_cipher_mapping() {
        assert!(valid_key_material(cipher::CCMP, 16));
        assert!(!valid_key_material(cipher::CCMP, 32));
        assert_eq!(cipher_suite(cipher::CCMP, 16), Some(0x000f_ac04));
        assert_eq!(cipher_suite(cipher::BIP_GMAC_256, 32), Some(0x000f_ac0c));
        assert_eq!(cipher_suite(0xff, 16), None);
    }

    #[test]
    fn translates_pairwise_ccmp_key_into_wal_contract() {
        let key = Key {
            abi_version: ABI_VERSION,
            cipher: cipher::CCMP,
            key_index: 0,
            flags: key_flag::RX | key_flag::TX | key_flag::PAIRWISE,
            peer: [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc],
            peer_present: 1,
            sequence_len: 6,
            sequence: [0; ws63_radio_sys::supplicant::KEY_SEQUENCE_LEN],
        };
        let mut material = [0x5a; 16];
        let request = key_request(&key, material.as_mut_ptr(), material.len()).unwrap();

        assert_eq!(request.key_type, KEY_TYPE_PAIRWISE);
        assert_eq!(request.cipher, 0x000f_ac04);
        assert_eq!(request.key_len, 16);
        assert_eq!(request.sequence_len, 6);
        assert_eq!(request.address, key.peer.as_ptr().cast_mut());
        assert_eq!(request.default_data, 1);
        assert_eq!(request.default_management, 0);
        assert_eq!(request.default_types, KEY_DEFAULT_UNICAST);
    }

    #[test]
    fn rejects_ambiguous_or_private_key_flags() {
        let mut key = Key {
            abi_version: ABI_VERSION,
            cipher: cipher::CCMP,
            key_index: 0,
            flags: key_flag::RX | key_flag::TX | key_flag::PAIRWISE | key_flag::GROUP,
            peer: [0; 6],
            peer_present: 1,
            sequence_len: 0,
            sequence: [0; ws63_radio_sys::supplicant::KEY_SEQUENCE_LEN],
        };
        assert!(key_request(&key, core::ptr::null_mut(), 0).is_none());

        key.flags = key_flag::RX | key_flag::TX | key_flag::PAIRWISE | key_flag::PMK;
        assert!(key_request(&key, core::ptr::null_mut(), 0).is_none());
    }
}
