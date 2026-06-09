//! Minimal libc symbols the vendor blobs reference but `compiler_builtins`
//! does not provide (string/heap/char). Heap routines back onto the same pool
//! as [`osal_kmalloc`](crate::alloc); string routines reuse the `osal_*`
//! implementations in [`crate::osal_ext`] where one already exists.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::{c_char, c_int, c_long, c_ulong, c_void};

// ── Heap ─────────────────────────────────────────────────────────────────────

/// `malloc`.
#[unsafe(no_mangle)]
pub extern "C" fn malloc(size: c_ulong) -> *mut c_void {
    crate::alloc::osal_kmalloc(size as usize)
}

/// `free`.
#[unsafe(no_mangle)]
pub extern "C" fn free(ptr: *mut c_void) {
    crate::alloc::osal_kfree(ptr);
}

/// `memalign`. NOTE: the backing heap returns 8-byte-aligned blocks; stricter
/// `alignment` (e.g. 64-byte DMA) is NOT yet honoured — a real aligned
/// allocator is a TODO before any DMA buffer is sourced through here.
#[unsafe(no_mangle)]
pub extern "C" fn memalign(_alignment: c_ulong, size: c_ulong) -> *mut c_void {
    crate::alloc::osal_kmalloc(size as usize)
}

// ── Strings ──────────────────────────────────────────────────────────────────

/// `strcmp` (delegates to the OSAL implementation).
#[unsafe(no_mangle)]
pub extern "C" fn strcmp(s1: *const c_char, s2: *const c_char) -> c_int {
    crate::osal_ext::osal_strcmp(s1, s2)
}

/// `strtol` (delegates to the OSAL implementation).
#[unsafe(no_mangle)]
pub extern "C" fn strtol(
    cp: *const c_char,
    endp: *mut *mut c_char,
    base: core::ffi::c_uint,
) -> c_long {
    crate::osal_ext::osal_strtol(cp, endp, base)
}

/// `atoi` — base-10 `strtol`, truncated to `int`.
#[unsafe(no_mangle)]
pub extern "C" fn atoi(s: *const c_char) -> c_int {
    crate::osal_ext::osal_strtol(s, core::ptr::null_mut(), 10) as c_int
}

/// `strstr` — first occurrence of `needle` in `haystack` (NULL if absent).
#[unsafe(no_mangle)]
pub extern "C" fn strstr(haystack: *const c_char, needle: *const c_char) -> *mut c_char {
    if haystack.is_null() || needle.is_null() {
        return core::ptr::null_mut();
    }
    let (h, n) = (haystack.cast::<u8>(), needle.cast::<u8>());
    // SAFETY: NUL-terminated C strings.
    let byte = |p: *const u8, i: usize| unsafe { p.add(i).read() };
    if byte(n, 0) == 0 {
        return haystack as *mut c_char; // empty needle matches at start
    }
    let mut i = 0usize;
    while byte(h, i) != 0 {
        let mut j = 0usize;
        while byte(n, j) != 0 && byte(h, i + j) == byte(n, j) {
            j += 1;
        }
        if byte(n, j) == 0 {
            // SAFETY: i is within the haystack we just scanned.
            return unsafe { h.add(i) } as *mut c_char;
        }
        i += 1;
    }
    core::ptr::null_mut()
}

/// `tolower` (ASCII).
#[unsafe(no_mangle)]
pub extern "C" fn tolower(c: c_int) -> c_int {
    if (b'A' as c_int..=b'Z' as c_int).contains(&c) {
        c + 32
    } else {
        c
    }
}

// ── Misc ─────────────────────────────────────────────────────────────────────

/// `gettimeofday(struct timeval *tv, void *tz)` — `timeval` matches
/// [`OsalTimeval`](crate::osal_ext::OsalTimeval); timezone is ignored.
#[unsafe(no_mangle)]
pub extern "C" fn gettimeofday(tv: *mut crate::osal_ext::OsalTimeval, _tz: *mut c_void) -> c_int {
    crate::osal_ext::osal_gettimeofday(tv);
    0
}

/// `print_str` — emit a C string to the log sink.
#[unsafe(no_mangle)]
pub extern "C" fn print_str(s: *const c_char) {
    crate::log::osal_printk(s);
}

/// `panic` — fatal error from the blob. Emit a marker and halt (a real handler
/// would reset; the stack here is not run on hardware yet).
#[unsafe(no_mangle)]
pub extern "C" fn panic() -> ! {
    crate::log_emit(b"[blob] panic\r\n");
    loop {
        core::hint::spin_loop();
    }
}
