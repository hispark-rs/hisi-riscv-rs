//! Remaining OSAL leaf primitives: heap aliases, string/mem helpers, time.
//!
//! Signatures from the C SDK `kernel/osal` headers (the deeper OSAL the WiFi
//! blob uses, beyond `ws63-RF/port_*.h`). All real — no scheduler needed.

// C-ABI entry points: the blob passes valid pointers (each still null-checks
// where it matters); marking them `unsafe` would not change the C symbol.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};

// ── Heap (vmalloc/vfree == kmalloc/kfree) ───────────────────────────────────

/// Allocate virtual memory (same backing heap as `osal_kmalloc`).
#[unsafe(no_mangle)]
pub extern "C" fn osal_vmalloc(size: c_ulong) -> *mut c_void {
    crate::alloc::osal_kmalloc(size as usize)
}
/// Free memory from [`osal_vmalloc`].
#[unsafe(no_mangle)]
pub extern "C" fn osal_vfree(addr: *mut c_void) {
    crate::alloc::osal_kfree(addr);
}

// ── String / memory helpers (core) ──────────────────────────────────────────

fn strlen(p: *const c_char) -> usize {
    if p.is_null() {
        return 0;
    }
    let p = p.cast::<u8>();
    let mut n = 0usize;
    // SAFETY: contract is a NUL-terminated C string.
    while unsafe { p.add(n).read() } != 0 {
        n += 1;
    }
    n
}

/// `strlen`.
#[unsafe(no_mangle)]
pub extern "C" fn osal_strlen(s: *const c_char) -> c_uint {
    strlen(s) as c_uint
}

/// `strcmp`.
#[unsafe(no_mangle)]
pub extern "C" fn osal_strcmp(s1: *const c_char, s2: *const c_char) -> c_int {
    if s1.is_null() || s2.is_null() {
        return (s1 as isize - s2 as isize).signum() as c_int;
    }
    let (a, b) = (s1.cast::<u8>(), s2.cast::<u8>());
    let mut i = 0usize;
    loop {
        // SAFETY: NUL-terminated C strings.
        let (ca, cb) = unsafe { (a.add(i).read(), b.add(i).read()) };
        if ca != cb {
            return ca as c_int - cb as c_int;
        }
        if ca == 0 {
            return 0;
        }
        i += 1;
    }
}

/// `strncmp` (adapt alias).
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_strncmp(s1: *const c_char, s2: *const c_char, n: c_uint) -> c_int {
    if s1.is_null() || s2.is_null() {
        return (s1 as isize - s2 as isize).signum() as c_int;
    }
    let (a, b) = (s1.cast::<u8>(), s2.cast::<u8>());
    for i in 0..n as usize {
        // SAFETY: bounded by n; NUL-terminated C strings.
        let (ca, cb) = unsafe { (a.add(i).read(), b.add(i).read()) };
        if ca != cb {
            return ca as c_int - cb as c_int;
        }
        if ca == 0 {
            break;
        }
    }
    0
}

/// `memcmp`.
#[unsafe(no_mangle)]
pub extern "C" fn osal_memcmp(cs: *const c_void, ct: *const c_void, count: c_int) -> c_int {
    if cs.is_null() || ct.is_null() || count <= 0 {
        return 0;
    }
    let (a, b) = (cs.cast::<u8>(), ct.cast::<u8>());
    for i in 0..count as usize {
        // SAFETY: bounded by count; caller guarantees both buffers.
        let (ca, cb) = unsafe { (a.add(i).read(), b.add(i).read()) };
        if ca != cb {
            return ca as c_int - cb as c_int;
        }
    }
    0
}

/// `strtol` (signed, base 0/2..36; minimal — no overflow saturation beyond i32).
#[unsafe(no_mangle)]
pub extern "C" fn osal_strtol(cp: *const c_char, endp: *mut *mut c_char, base: c_uint) -> c_long {
    if cp.is_null() {
        return 0;
    }
    let p = cp.cast::<u8>();
    let mut i = 0usize;
    // SAFETY: NUL-terminated C string.
    let rd = |k: usize| unsafe { p.add(k).read() };
    while matches!(rd(i), b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    let neg = match rd(i) {
        b'-' => {
            i += 1;
            true
        }
        b'+' => {
            i += 1;
            false
        }
        _ => false,
    };
    let mut radix = base;
    if radix == 0 {
        if rd(i) == b'0' && matches!(rd(i + 1), b'x' | b'X') {
            radix = 16;
            i += 2;
        } else if rd(i) == b'0' {
            radix = 8;
        } else {
            radix = 10;
        }
    } else if radix == 16 && rd(i) == b'0' && matches!(rd(i + 1), b'x' | b'X') {
        i += 2;
    }
    let mut acc: i64 = 0;
    loop {
        let c = rd(i);
        let d = match c {
            b'0'..=b'9' => (c - b'0') as u32,
            b'a'..=b'z' => (c - b'a' + 10) as u32,
            b'A'..=b'Z' => (c - b'A' + 10) as u32,
            _ => break,
        };
        if d >= radix {
            break;
        }
        acc = acc * radix as i64 + d as i64;
        i += 1;
    }
    if !endp.is_null() {
        // SAFETY: endp is a valid out-parameter when non-null.
        unsafe { *endp = p.add(i) as *mut c_char };
    }
    let v = if neg { -acc } else { acc };
    v as c_long
}

// ── Time ────────────────────────────────────────────────────────────────────

/// Monotonic tick count; 1 jiffy is one mask-ROM systick millisecond.
#[unsafe(no_mangle)]
pub extern "C" fn osal_get_jiffies() -> u64 {
    crate::uapi::monotonic_ms()
}
/// Convert jiffies to milliseconds (1:1 here).
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_jiffies_to_msecs(jiffies: c_ulong) -> c_ulong {
    jiffies
}

/// Mirrors C `osal_timeval { long tv_sec; long tv_usec; }`.
#[repr(C)]
pub struct OsalTimeval {
    tv_sec: c_long,
    tv_usec: c_long,
}
/// Fill `tv` with time since boot from the mask-ROM TCXO counter.
#[unsafe(no_mangle)]
pub extern "C" fn osal_gettimeofday(tv: *mut OsalTimeval) {
    if tv.is_null() {
        return;
    }
    let us = crate::uapi::monotonic_us();
    // SAFETY: tv is a valid out-parameter.
    unsafe {
        (*tv).tv_sec = (us / 1_000_000) as c_long;
        (*tv).tv_usec = (us % 1_000_000) as c_long;
    }
}

// (`osal_get_current_tid` is defined in `osal.rs` alongside `osal_get_current_pid`.)

/// Current task id (adapt alias).
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_get_current_tid() -> c_long {
    crate::runtime::current_id() as c_long
}

/// Copy to "user" memory (flat address space — a plain memcpy). Returns the
/// number of bytes NOT copied (0 = all copied).
#[unsafe(no_mangle)]
pub extern "C" fn osal_copy_to_user(to: *mut c_void, from: *const c_void, n: c_ulong) -> c_ulong {
    if to.is_null() || from.is_null() {
        return n;
    }
    // SAFETY: caller guarantees both regions for n bytes.
    unsafe { core::ptr::copy_nonoverlapping(from.cast::<u8>(), to.cast::<u8>(), n as usize) };
    0
}
