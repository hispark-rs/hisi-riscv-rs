//! Native runtime hooks for the pinned upstream hostap port.
//!
//! The C port owns no scheduler and does not emulate LiteOS. It delegates only
//! the capabilities needed by the single `RadioRunner` through
//! `hisi-rf-rtos-driver`, the RF heap, the WS63 monotonic clock and the explicit
//! WS63 entropy backend.

use core::ffi::{c_int, c_void};
use core::num::NonZeroU32;

use hisi_rf_rtos_driver::{Semaphore, WaitOutcome, WaitTimeout};
use portable_atomic::{AtomicBool, Ordering};
use ws63_radio_sys::supplicant::{
    ABI_VERSION, DriverHooks, Key, OsHooks, hisi_wpa_driver_install, hisi_wpa_os_install,
    hisi_wpa_os_uninstall,
};

static RUNNER_WAKE: Semaphore = Semaphore::new(0);
static PORT_IDENTITY: u8 = 0;
static PORT_CLAIMED: AtomicBool = AtomicBool::new(false);

const ETHERNET_HEADER_LEN: usize = 14;
const EAPOL_ETHERTYPE: [u8; 2] = [0x88, 0x8e];
// The Personal-only vendor profile uses EAPOL_PKT_BUF_SIZE=800. Enterprise is
// a separate future profile and must raise this bound explicitly.
const MAX_EAPOL_PAYLOAD_LEN: usize = 800;

/// Failure while registering the upstream supplicant native runtime seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpstreamSupplicantPortError {
    /// The application has not installed a runtime or exhausted its resources.
    Runtime(hisi_rf_rtos_driver::Error),
    /// The C ABI rejected the hook table or another runtime already owns it.
    Abi(i32),
}

/// Install the native OS/eloop hooks after `hisi-rtos` has installed its RF
/// runtime contract and after the WS63 ROM timebases are initialized.
///
/// The C boundary copies the hook table synchronously, so no Rust reference is
/// retained. Calling this repeatedly with the same hooks is idempotent.
pub fn prepare_upstream_supplicant_port() -> Result<(), UpstreamSupplicantPortError> {
    RUNNER_WAKE
        .try_init()
        .map_err(UpstreamSupplicantPortError::Runtime)?;

    if PORT_CLAIMED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Ok(());
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
        driver: core::ptr::addr_of!(PORT_IDENTITY).cast_mut().cast(),
        get_own_address: Some(get_own_address),
        send_eapol: Some(send_eapol),
        send_mgmt: Some(send_mgmt_unavailable),
        install_key: Some(install_key_unavailable),
        remove_key: Some(remove_key_unavailable),
    };
    // SAFETY: `os_hooks` matches ABI_VERSION and the C function copies the table
    // before returning. Every callback has C ABI and static backing state.
    let os_result = unsafe { hisi_wpa_os_install(&raw const os_hooks) };
    if os_result != 0 {
        PORT_CLAIMED.store(false, Ordering::Release);
        return Err(UpstreamSupplicantPortError::Abi(os_result));
    }
    // SAFETY: the versioned table contains only C ABI callbacks with static
    // backing state and is copied by the C boundary before returning.
    let driver_result = unsafe { hisi_wpa_driver_install(&raw const driver_hooks) };
    if driver_result == 0 {
        return Ok(());
    }

    // No driver/L2 user can exist before driver registration succeeds, so the
    // OS registration made above can be rolled back synchronously.
    let rollback = unsafe { hisi_wpa_os_uninstall(os_hooks.context) };
    if rollback == 0 {
        PORT_CLAIMED.store(false, Ordering::Release);
    }
    Err(UpstreamSupplicantPortError::Abi(driver_result))
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

unsafe extern "C" fn get_own_address(_: *mut c_void, address: *mut u8) -> c_int {
    if address.is_null() {
        return -1;
    }
    if crate::uapi::get_dev_addr(address, 6, 2) == crate::OSAL_OK as u32 {
        0
    } else {
        -1
    }
}

unsafe extern "C" fn send_eapol(
    _: *mut c_void,
    destination: *const u8,
    payload: *const u8,
    payload_len: usize,
) -> c_int {
    if destination.is_null()
        || payload.is_null()
        || payload_len == 0
        || payload_len > MAX_EAPOL_PAYLOAD_LEN
    {
        return -1;
    }
    let mut source = [0; 6];
    if crate::uapi::get_dev_addr(source.as_mut_ptr(), 6, 2) != crate::OSAL_OK as u32 {
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
    crate::netif::transmit(frame).map(|()| 0).unwrap_or(-1)
}

unsafe extern "C" fn send_mgmt_unavailable(
    _: *mut c_void,
    _: u32,
    _: *const u8,
    _: usize,
) -> c_int {
    -1
}

unsafe extern "C" fn install_key_unavailable(
    _: *mut c_void,
    _: *const Key,
    _: *const u8,
    _: usize,
) -> c_int {
    -1
}

unsafe extern "C" fn remove_key_unavailable(_: *mut c_void, _: *const Key) -> c_int {
    -1
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
            prepare_upstream_supplicant_port(),
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
}
