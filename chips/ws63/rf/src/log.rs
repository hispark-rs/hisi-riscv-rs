//! Logging + safe C-library shims (ws63-RF `port_log.h`).
//!
//! The Wi-Fi `log_event_wifi_print*` functions take packed diagnostic words,
//! not C string pointers. The first word is `presspara` from the vendor
//! `para_press(LOG_WIFIMODULE, lvl, THIS_FILE_ID, __LINE__)` macro. Do not
//! dereference it. We emit compact hex diagnostics to the installed sink.
//!
//! `osal_printk` / `snprintf_s` still use C strings. The `snprintf_s` adapter
//! implements the bounded conversion subset used by the vendor control path.
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

unsafe fn vsnprintf_subset(
    buf: *mut c_char,
    size: usize,
    count: usize,
    fmt: *const c_char,
    mut arguments: core::ffi::VaList<'_>,
) -> c_int {
    if buf.is_null() || size == 0 {
        return -1;
    }
    let src = cstr_bytes(fmt);
    let limit = core::cmp::min(size - 1, count);
    let output = buf.cast::<u8>();
    let mut input_index = 0;
    let mut output_index = 0;

    let write_bytes = |bytes: &[u8], output_index: &mut usize| -> bool {
        if *output_index + bytes.len() > limit {
            return false;
        }
        for &byte in bytes {
            // SAFETY: the bounds check above keeps every write below size.
            unsafe { output.add(*output_index).write(byte) };
            *output_index += 1;
        }
        true
    };

    while input_index < src.len() {
        let mut conversion_end = input_index + 1;
        let mut zero_pad = false;
        let mut width = 0usize;
        if src[input_index] == b'%' {
            if src.get(conversion_end) == Some(&b'0') {
                zero_pad = true;
                conversion_end += 1;
            }
            while let Some(digit @ b'0'..=b'9') = src.get(conversion_end).copied() {
                width = width
                    .saturating_mul(10)
                    .saturating_add((digit - b'0') as usize);
                conversion_end += 1;
            }
        }
        let conversion = src.get(conversion_end).copied();
        if src[input_index] == b'%' && matches!(conversion, Some(b's' | b'u' | b'd' | b'x')) {
            let specifier = conversion.unwrap_or_default();
            if specifier == b's' {
                // SAFETY: the C caller must match `%s` with a promoted pointer.
                let argument = unsafe { arguments.next_arg::<*const c_char>() };
                let bytes = cstr_bytes(argument);
                if !write_bytes(bytes, &mut output_index) {
                    // SAFETY: `size > 0`, so the first destination byte exists.
                    unsafe { output.write(0) };
                    return -1;
                }
                input_index = conversion_end + 1;
                continue;
            }

            let mut digits = [0_u8; 10];
            // SAFETY: C default argument promotions pass integer conversions as
            // `c_int`/`c_uint`; the format string selects the matching type.
            let (argument, negative) = if specifier == b'd' {
                let signed = unsafe { arguments.next_arg::<c_int>() };
                (signed.unsigned_abs(), signed < 0)
            } else {
                (unsafe { arguments.next_arg::<c_uint>() }, false)
            };
            let mut value = argument;
            let radix = if specifier == b'x' { 16 } else { 10 };
            let mut digits_len = 0;
            loop {
                let digit = (value % radix) as u8;
                digits[digits_len] = if digit < 10 {
                    b'0' + digit
                } else {
                    b'a' + digit - 10
                };
                digits_len += 1;
                value /= radix;
                if value == 0 {
                    break;
                }
            }
            let rendered_len = (digits_len + usize::from(negative)).max(width);
            if output_index + rendered_len > limit {
                // SAFETY: `size > 0`, so the first destination byte exists.
                unsafe { output.write(0) };
                return -1;
            }
            if negative {
                // SAFETY: included in the bounds check above.
                unsafe { output.add(output_index).write(b'-') };
                output_index += 1;
            }
            let padding = rendered_len - digits_len - usize::from(negative);
            for _ in 0..padding {
                // SAFETY: padding is included in the bounds check above.
                unsafe {
                    output
                        .add(output_index)
                        .write(if zero_pad { b'0' } else { b' ' })
                };
                output_index += 1;
            }
            for digit in digits[..digits_len].iter().rev() {
                // SAFETY: the bounds check above keeps every write below size.
                unsafe { output.add(output_index).write(*digit) };
                output_index += 1;
            }
            input_index = conversion_end + 1;
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

/// Debug printf (OSAL), including its C variadic arguments.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn osal_printk(fmt: *const c_char, arguments: ...) -> c_int {
    let mut output = [0 as c_char; 512];
    // SAFETY: the vendor caller's format string defines the variadic ABI.
    let written = unsafe {
        vsnprintf_subset(
            output.as_mut_ptr(),
            output.len(),
            output.len() - 1,
            fmt,
            arguments,
        )
    };
    if written >= 0 {
        log_emit(cstr_bytes(output.as_ptr()));
    } else {
        log_emit(b"[osal_printk truncated] ");
        log_emit(cstr_bytes(fmt));
    }
    written
}

/// Bounded `snprintf_s` subset used by the vendor Wi-Fi objects.
///
/// The implementation consumes a real C [`core::ffi::VaList`], so arguments
/// beyond `a7` remain ABI-correct. Supported conversions are `%s`, `%d`, `%u`,
/// `%x`, `%%`, and integer field widths including the vendor `MACSTR` `%02x`.
/// Unsupported conversion specifiers are copied literally without consuming
/// an argument. Returns bytes written (excluding NUL), or `-1` after clearing
/// the destination when the result would be truncated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snprintf_s(
    buf: *mut c_char,
    size: usize,
    count: usize,
    fmt: *const c_char,
    arguments: ...
) -> c_int {
    // SAFETY: the vendor caller's format string defines the variadic ABI.
    unsafe { vsnprintf_subset(buf, size, count, fmt, arguments) }
}

#[cfg(test)]
mod snprintf_tests {
    use super::snprintf_s;

    #[test]
    fn expands_vendor_netdev_id() {
        let mut output = [0_i8; 16];
        let result = unsafe {
            snprintf_s(
                output.as_mut_ptr(),
                output.len(),
                output.len(),
                c"Featureid%u".as_ptr(),
                0_u32,
            )
        };
        let bytes = output.map(|byte| byte as u8);
        assert_eq!(result, 10);
        assert_eq!(&bytes[..11], b"Featureid0\0");
    }

    #[test]
    fn rejects_truncated_output() {
        let mut output = [1_i8; 8];
        assert_eq!(
            unsafe {
                snprintf_s(
                    output.as_mut_ptr(),
                    output.len(),
                    output.len(),
                    c"Featureid%u".as_ptr(),
                    12_u32,
                )
            },
            -1
        );
        assert_eq!(output[0], 0);
    }

    #[test]
    fn expands_vendor_mac_address() {
        let mut output = [0_i8; 18];
        let result = unsafe {
            snprintf_s(
                output.as_mut_ptr(),
                output.len(),
                output.len() - 1,
                c"%02x:%02x:%02x:%02x:%02x:%02x".as_ptr(),
                0x82_u32,
                0x2e_u32,
                0xb3_u32,
                0xc1_u32,
                0x55_u32,
                0xc4_u32,
            )
        };
        let bytes = output.map(|byte| byte as u8);
        assert_eq!(result, 17);
        assert_eq!(&bytes, b"82:2e:b3:c1:55:c4\0");
    }
}

/// Safe memset (securec): refuses if `count > dest_max`.
#[unsafe(no_mangle)]
pub extern "C" fn memset_s(dest: *mut c_void, dest_max: usize, c: c_int, count: usize) -> c_int {
    #[cfg(all(feature = "rf-eloop-diag", target_arch = "riscv32"))]
    {
        let caller: usize;
        let command: usize;
        let length: usize;
        // SAFETY: these moves only snapshot registers. s2/s4 are callee-saved,
        // so they still contain the vendor dispatcher's live switch operands.
        unsafe {
            core::arch::asm!(
                "mv {caller}, ra",
                "mv {command}, s2",
                "mv {length}, s4",
                caller = out(reg) caller,
                command = out(reg) command,
                length = out(reg) length,
                options(nomem, nostack),
            );
        }
        crate::eloop_diag::record_dispatch_registers(caller, command, length);
    }
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
