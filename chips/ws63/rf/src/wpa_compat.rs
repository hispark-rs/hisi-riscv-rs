//! Small service/kernel contracts required by the delivered WPA archive.

use core::ffi::{c_char, c_int, c_void};

/// Match the SDK's `wifi_is_need_psk`: open and OWE do not carry a PSK.
#[unsafe(no_mangle)]
pub extern "C" fn wifi_is_need_psk(security_type: c_int) -> c_int {
    i32::from(security_type != 0 && security_type != 10)
}

/// LiteOS recycles detached task resources here. Rust owns task slots directly.
#[unsafe(no_mangle)]
pub extern "C" fn LOS_TaskResRecycle() {}

/// Forward the WPA formatter's static format string to the configured RF log
/// sink. Formatting the `va_list` remains libc-owned; retaining the message
/// text is enough to identify the failing vendor path without a second printf.
#[unsafe(no_mangle)]
pub extern "C" fn UartVprintf(fmt: *const c_char, _args: *mut c_void) -> c_int {
    // SAFETY: this compatibility path forwards only the static format string.
    unsafe { crate::log::osal_printk(fmt) };
    0
}

/// LiteOS interrupt-context query used only by libc errno selection.
#[unsafe(no_mangle)]
pub extern "C" fn IntActive() -> c_int {
    0
}

/// Current LiteOS task ID compatibility hook.
#[unsafe(no_mangle)]
pub extern "C" fn LOS_CurTaskIDGet() -> u32 {
    crate::runtime::current_id() as u32
}

/// Dummy LiteOS pool anchor; the compatibility allocators ignore the pool.
#[unsafe(no_mangle)]
pub static mut g_intheap_begin: u8 = 0;

#[unsafe(no_mangle)]
pub extern "C" fn LOS_MemAlloc(_pool: *mut c_void, size: u32) -> *mut c_void {
    crate::alloc::osal_kmalloc(size as usize)
}

#[unsafe(no_mangle)]
pub extern "C" fn LOS_MemFree(_pool: *mut c_void, ptr: *mut c_void) -> u32 {
    crate::alloc::osal_kfree(ptr);
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn LOS_MemRealloc(_pool: *mut c_void, ptr: *mut c_void, size: u32) -> *mut c_void {
    crate::alloc::realloc_owned(ptr, size as usize)
}
