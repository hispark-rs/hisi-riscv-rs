//! Native runtime hooks for the pinned upstream hostap port.
//!
//! The C port owns no scheduler and does not emulate LiteOS. It delegates only
//! the capabilities needed by the single `RadioRunner` through
//! `hisi-rf-rtos-driver`, the RF heap, the WS63 monotonic clock and the explicit
//! WS63 entropy backend.

use core::ffi::{c_int, c_void};
use core::num::NonZeroU32;

use hisi_rf_rtos_driver::{Semaphore, WaitOutcome, WaitTimeout};
use ws63_radio_sys::supplicant::{ABI_VERSION, OsHooks, hisi_wpa_os_install};

static RUNNER_WAKE: Semaphore = Semaphore::new(0);
static PORT_IDENTITY: u8 = 0;

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

    let hooks = OsHooks {
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
    // SAFETY: `hooks` matches ABI_VERSION and the C function copies the table
    // before returning. Every callback has C ABI and static backing state.
    let result = unsafe { hisi_wpa_os_install(&raw const hooks) };
    if result == 0 {
        Ok(())
    } else {
        Err(UpstreamSupplicantPortError::Abi(result))
    }
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
}
