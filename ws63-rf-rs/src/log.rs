//! Logging + safe C-library shims (ws63-RF `port_log.h`).
//!
//! The `*_print*` / `osal_printk` / `snprintf_s` functions are C variadics.
//! We declare them as taking only the fixed `fmt` argument: on the RISC-V
//! ilp32 ABI the caller passes varargs in a1.. / on the stack and cleans up
//! itself, so a callee that reads only `fmt` (a0) is ABI-safe. Consequently we
//! emit the **raw format string** to the log sink WITHOUT expanding `%`
//! specifiers (a real `vsnprintf` is a TODO; the Wi-Fi stack is not run yet).
//!
//! `memset_s` / `memcpy_s` are NOT variadic and ARE used for real memory moves
//! by the blobs, so they are implemented faithfully (securec semantics:
//! return 0 on success, non-zero on a bounds violation, and do not write past
//! `dest_max`).

use crate::log_emit;
use core::ffi::{c_char, c_int, c_void};

/// Bounded `strlen` for a C string (capped so a stray pointer can't run away).
fn cstr_bytes<'a>(p: *const c_char) -> &'a [u8] {
    const MAX: usize = 256;
    if p.is_null() {
        return &[];
    }
    let p = p.cast::<u8>();
    let mut n = 0usize;
    // SAFETY: bounded scan; callers pass NUL-terminated C strings.
    while n < MAX && unsafe { p.add(n).read() } != 0 {
        n += 1;
    }
    unsafe { core::slice::from_raw_parts(p, n) }
}

fn emit_line(level: &[u8], fmt: *const c_char) {
    log_emit(level);
    log_emit(cstr_bytes(fmt));
    log_emit(b"\r\n");
}

/// WiFi diagnostic print, level 0 = error/fatal.
#[unsafe(no_mangle)]
pub extern "C" fn log_event_wifi_print0(fmt: *const c_char) -> c_int {
    emit_line(b"[wifi:E] ", fmt);
    0
}
/// WiFi diagnostic print, level 1 = warning.
#[unsafe(no_mangle)]
pub extern "C" fn log_event_wifi_print1(fmt: *const c_char) -> c_int {
    emit_line(b"[wifi:W] ", fmt);
    0
}
/// WiFi diagnostic print, level 2 = info.
#[unsafe(no_mangle)]
pub extern "C" fn log_event_wifi_print2(fmt: *const c_char) -> c_int {
    emit_line(b"[wifi:I] ", fmt);
    0
}
/// WiFi diagnostic print, level 3 = debug. (Declared by `port_log.h` only as
/// 0/1/2/4, but `libwifi_driver_dmac.a` also references print3 — verified by nm.)
#[unsafe(no_mangle)]
pub extern "C" fn log_event_wifi_print3(fmt: *const c_char) -> c_int {
    emit_line(b"[wifi:D] ", fmt);
    0
}
/// WiFi diagnostic print, level 4 = verbose/trace.
#[unsafe(no_mangle)]
pub extern "C" fn log_event_wifi_print4(fmt: *const c_char) -> c_int {
    emit_line(b"[wifi:V] ", fmt);
    0
}

// Generic-module diagnostic prints (BT / GNSS / platform …). Unlike the wifi
// variants, the first argument is a packed `log_head` word, not a format
// string, so there is nothing safe to render — they swallow the event and
// return 0 (ABI-safe: extra args sit in a1.. and the caller cleans up).
/// Generic log event, 0 format args.
#[unsafe(no_mangle)]
pub extern "C" fn log_event_print0() -> c_int {
    0
}
/// Generic log event, 1 format arg.
#[unsafe(no_mangle)]
pub extern "C" fn log_event_print1() -> c_int {
    0
}
/// Generic log event, 2 format args.
#[unsafe(no_mangle)]
pub extern "C" fn log_event_print2() -> c_int {
    0
}
/// Generic log event, 3 format args.
#[unsafe(no_mangle)]
pub extern "C" fn log_event_print3() -> c_int {
    0
}
/// Generic log event, 4 format args.
#[unsafe(no_mangle)]
pub extern "C" fn log_event_print4() -> c_int {
    0
}

/// Debug printf (OSAL). Emits the raw format string to the log sink.
#[unsafe(no_mangle)]
pub extern "C" fn osal_printk(fmt: *const c_char) -> c_int {
    log_emit(cstr_bytes(fmt));
    0
}

/// Safe snprintf. SCAFFOLD: copies the format string verbatim (no `%`
/// expansion — TODO: real vsnprintf). Returns bytes written (excluding NUL).
#[unsafe(no_mangle)]
pub extern "C" fn snprintf_s(buf: *mut c_char, size: usize, fmt: *const c_char) -> c_int {
    if buf.is_null() || size == 0 {
        return -1;
    }
    let src = cstr_bytes(fmt);
    let n = core::cmp::min(src.len(), size - 1);
    // SAFETY: writing n+1 (<= size) bytes into buf.
    unsafe {
        core::ptr::copy_nonoverlapping(src.as_ptr(), buf.cast::<u8>(), n);
        buf.cast::<u8>().add(n).write(0);
    }
    n as c_int
}

/// Safe memset (securec): refuses if `count > dest_max`.
#[unsafe(no_mangle)]
pub extern "C" fn memset_s(dest: *mut c_void, dest_max: usize, c: c_int, count: usize) -> c_int {
    if dest.is_null() || count > dest_max {
        return crate::OSAL_NOK;
    }
    // SAFETY: count <= dest_max bytes of dest are writable.
    unsafe { core::ptr::write_bytes(dest as *mut u8, c as u8, count) };
    crate::OSAL_OK
}

/// Safe memcpy (securec): refuses if `count > dest_max`.
#[unsafe(no_mangle)]
pub extern "C" fn memcpy_s(
    dest: *mut c_void,
    dest_max: usize,
    src: *const c_void,
    count: usize,
) -> c_int {
    if dest.is_null() || src.is_null() || count > dest_max {
        return crate::OSAL_NOK;
    }
    // SAFETY: count <= dest_max bytes; src is assumed valid for count bytes.
    unsafe { core::ptr::copy_nonoverlapping(src as *const u8, dest as *mut u8, count) };
    crate::OSAL_OK
}
