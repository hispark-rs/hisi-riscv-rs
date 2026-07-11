//! Logging + safe C-library shims (ws63-RF `port_log.h`).
//!
//! The Wi-Fi `log_event_wifi_print*` functions take packed diagnostic words,
//! not C string pointers. The first word is `presspara` from the vendor
//! `para_press(LOG_WIFIMODULE, lvl, THIS_FILE_ID, __LINE__)` macro. Do not
//! dereference it. We emit compact hex diagnostics to the installed sink.
//!
//! `osal_printk` / `snprintf_s` still use C strings. The `snprintf_s` adapter
//! implements the single-argument `%u` form used to create vendor netdev names.
//!
//! `memset_s` / `memcpy_s` are NOT variadic and ARE used for real memory moves
//! by the blobs, so they are implemented faithfully (securec semantics:
//! return 0 on success, non-zero on a bounds violation, and do not write past
//! `dest_max`).

use crate::log_emit;
use core::ffi::{c_char, c_int, c_uint, c_void};

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

fn emit_hex32(value: c_uint) {
    let mut buf = *b"0x00000000";
    let mut i = 0;
    while i < 8 {
        let nib = ((value >> ((7 - i) * 4)) & 0xf) as u8;
        buf[2 + i] = if nib < 10 {
            b'0' + nib
        } else {
            b'a' + (nib - 10)
        };
        i += 1;
    }
    log_emit(&buf);
}

fn emit_wifi_event(presspara: c_uint, args: &[c_uint]) {
    let level = match (presspara >> 6) & 0x3 {
        1 => b"[wifi:E] ".as_slice(),
        2 => b"[wifi:W] ".as_slice(),
        3 => b"[wifi:I] ".as_slice(),
        _ => b"[wifi:?] ".as_slice(),
    };
    let file_id = ((presspara >> 6) & 0x3fc) | ((presspara >> 16) & 0x3);
    let line = ((presspara >> 24) & 0xff) | ((presspara >> 10) & 0x3f00);

    log_emit(level);
    log_emit(b"file=");
    emit_hex32(file_id);
    log_emit(b" line=");
    emit_hex32(line);
    log_emit(b" ");
    log_emit(b"press=");
    emit_hex32(presspara);
    for &arg in args {
        log_emit(b" ");
        emit_hex32(arg);
    }
    log_emit(b"\r\n");
}

/// Wi-Fi diagnostic event with no value arguments.
#[unsafe(no_mangle)]
pub extern "C" fn log_event_wifi_print0(presspara: c_uint) -> c_int {
    emit_wifi_event(presspara, &[]);
    0
}
/// Wi-Fi diagnostic event with one value argument.
#[unsafe(no_mangle)]
pub extern "C" fn log_event_wifi_print1(presspara: c_uint, para1: c_uint) -> c_int {
    emit_wifi_event(presspara, &[para1]);
    0
}
/// Wi-Fi diagnostic event with two value arguments.
#[unsafe(no_mangle)]
pub extern "C" fn log_event_wifi_print2(presspara: c_uint, para1: c_uint, para2: c_uint) -> c_int {
    emit_wifi_event(presspara, &[para1, para2]);
    0
}
/// Wi-Fi diagnostic event with three value arguments. (Declared by `port_log.h` only as
/// 0/1/2/4, but `libwifi_driver_dmac.a` also references print3 — verified by nm.)
#[unsafe(no_mangle)]
pub extern "C" fn log_event_wifi_print3(
    presspara: c_uint,
    para1: c_uint,
    para2: c_uint,
    para3: c_uint,
) -> c_int {
    emit_wifi_event(presspara, &[para1, para2, para3]);
    0
}
/// Wi-Fi diagnostic event with four value arguments.
#[unsafe(no_mangle)]
pub extern "C" fn log_event_wifi_print4(
    presspara: c_uint,
    para1: c_uint,
    para2: c_uint,
    para3: c_uint,
    para4: c_uint,
) -> c_int {
    emit_wifi_event(presspara, &[para1, para2, para3, para4]);
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

/// Bounded `snprintf_s` subset used by the vendor Wi-Fi objects.
///
/// The prebuilt objects currently require one unsigned argument for interface
/// names such as `Featureid%u`. The fifth C ABI argument is therefore modeled
/// explicitly; unsupported conversion specifiers are copied literally instead
/// of consuming an argument. Returns bytes written (excluding NUL), or `-1`
/// after clearing the destination when the result would be truncated.
#[unsafe(no_mangle)]
pub extern "C" fn snprintf_s(
    buf: *mut c_char,
    size: usize,
    count: usize,
    fmt: *const c_char,
    arg: c_uint,
) -> c_int {
    if buf.is_null() || size == 0 {
        return -1;
    }
    let src = cstr_bytes(fmt);
    let limit = core::cmp::min(size - 1, count);
    let output = buf.cast::<u8>();
    let mut input_index = 0;
    let mut output_index = 0;

    while input_index < src.len() {
        if src[input_index] == b'%' && src.get(input_index + 1) == Some(&b'u') {
            let mut digits = [0_u8; 10];
            let mut value = arg;
            let mut digits_len = 0;
            loop {
                digits[digits_len] = b'0' + (value % 10) as u8;
                digits_len += 1;
                value /= 10;
                if value == 0 {
                    break;
                }
            }
            if output_index + digits_len > limit {
                // SAFETY: `size > 0`, so the first destination byte exists.
                unsafe { output.write(0) };
                return -1;
            }
            for digit in digits[..digits_len].iter().rev() {
                // SAFETY: the bounds check above keeps every write below size.
                unsafe { output.add(output_index).write(*digit) };
                output_index += 1;
            }
            input_index += 2;
            continue;
        }

        let byte = if src[input_index] == b'%' && src.get(input_index + 1) == Some(&b'%') {
            input_index += 2;
            b'%'
        } else {
            let byte = src[input_index];
            input_index += 1;
            byte
        };
        if output_index == limit {
            // SAFETY: `size > 0`, so the first destination byte exists.
            unsafe { output.write(0) };
            return -1;
        }
        // SAFETY: output_index is below the effective size/count limit.
        unsafe { output.add(output_index).write(byte) };
        output_index += 1;
    }

    // SAFETY: output_index <= limit <= size - 1 leaves room for the terminator.
    unsafe { output.add(output_index).write(0) };
    output_index as c_int
}

#[cfg(test)]
mod snprintf_tests {
    use super::snprintf_s;

    #[test]
    fn expands_vendor_netdev_id() {
        let mut output = [0_i8; 16];
        let result = snprintf_s(
            output.as_mut_ptr(),
            output.len(),
            output.len(),
            c"Featureid%u".as_ptr(),
            0,
        );
        let bytes = output.map(|byte| byte as u8);
        assert_eq!(result, 10);
        assert_eq!(&bytes[..11], b"Featureid0\0");
    }

    #[test]
    fn rejects_truncated_output() {
        let mut output = [1_i8; 8];
        assert_eq!(
            snprintf_s(
                output.as_mut_ptr(),
                output.len(),
                output.len(),
                c"Featureid%u".as_ptr(),
                12,
            ),
            -1
        );
        assert_eq!(output[0], 0);
    }
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
