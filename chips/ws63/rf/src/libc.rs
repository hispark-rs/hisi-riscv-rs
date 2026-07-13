//! Minimal libc symbols the vendor blobs reference but `compiler_builtins`
//! does not provide (string/heap/char). Heap routines back onto the same pool
//! as [`osal_kmalloc`](crate::alloc); string routines reuse the `osal_*`
//! implementations in [`crate::osal_ext`] where one already exists.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::{c_char, c_int, c_long, c_ulong, c_void};

// These names deliberately do not collide with compiler-builtins' internal
// libc shims. The fixed-address mask-ROM veneer table points at them explicitly.

/// Mask-ROM `memset` veneer.
///
/// # Safety
///
/// `dest` must identify a writable region of at least `count` bytes.
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
pub unsafe extern "C" fn __ws63_rom_memset(
    dest: *mut c_void,
    value: c_int,
    count: usize,
) -> *mut c_void {
    if !dest.is_null() {
        // SAFETY: the mask-ROM caller owns a writable `count`-byte destination.
        unsafe { core::ptr::write_bytes(dest.cast::<u8>(), value as u8, count) };
    }
    dest
}

/// Mask-ROM `memcpy` veneer.
///
/// # Safety
///
/// `source` and `dest` must identify readable and writable regions of at
/// least `count` bytes, and the regions must not overlap.
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
pub unsafe extern "C" fn __ws63_rom_memcpy(
    dest: *mut c_void,
    source: *const c_void,
    count: usize,
) -> *mut c_void {
    if !dest.is_null() && !source.is_null() {
        // SAFETY: the C memcpy contract requires valid non-overlapping ranges.
        unsafe { core::ptr::copy_nonoverlapping(source.cast::<u8>(), dest.cast::<u8>(), count) };
    }
    dest
}

/// Mask-ROM `memmove` veneer.
///
/// # Safety
///
/// `source` and `dest` must identify readable and writable regions of at
/// least `count` bytes. The regions may overlap.
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
pub unsafe extern "C" fn __ws63_rom_memmove(
    dest: *mut c_void,
    source: *const c_void,
    count: usize,
) -> *mut c_void {
    if !dest.is_null() && !source.is_null() {
        // SAFETY: the C memmove contract permits overlapping valid ranges.
        unsafe { core::ptr::copy(source.cast::<u8>(), dest.cast::<u8>(), count) };
    }
    dest
}

/// Mask-ROM `memcmp` veneer.
///
/// # Safety
///
/// `left` and `right` must each identify a readable region of at least
/// `count` bytes.
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
pub unsafe extern "C" fn __ws63_rom_memcmp(
    left: *const c_void,
    right: *const c_void,
    count: usize,
) -> c_int {
    let mut index = 0;
    while index < count {
        // SAFETY: the C memcmp contract requires two readable `count`-byte ranges.
        let a = unsafe { left.cast::<u8>().add(index).read() };
        let b = unsafe { right.cast::<u8>().add(index).read() };
        if a != b {
            return a as c_int - b as c_int;
        }
        index += 1;
    }
    0
}

/// Mask-ROM `strlen` veneer.
///
/// # Safety
///
/// A non-null `value` must point to a readable NUL-terminated C string.
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
pub unsafe extern "C" fn __ws63_rom_strlen(value: *const c_char) -> usize {
    if value.is_null() {
        return 0;
    }
    let mut length = 0;
    // SAFETY: the C strlen contract requires a readable NUL-terminated string.
    while unsafe { value.add(length).read() } != 0 {
        length += 1;
    }
    length
}

// ── Heap ─────────────────────────────────────────────────────────────────────

/// `malloc`.
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
pub extern "C" fn malloc(size: c_ulong) -> *mut c_void {
    crate::alloc::osal_kmalloc(size as usize)
}

/// `free`.
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
pub extern "C" fn free(ptr: *mut c_void) {
    crate::alloc::osal_kfree(ptr);
}

/// `realloc` for allocations owned by the RF heap.
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
pub extern "C" fn realloc(ptr: *mut c_void, size: c_ulong) -> *mut c_void {
    crate::alloc::realloc_owned(ptr, size as usize)
}

/// `memalign`. NOTE: the backing heap returns 8-byte-aligned blocks; stricter
/// `alignment` (e.g. 64-byte DMA) is NOT yet honoured — a real aligned
/// allocator is a TODO before any DMA buffer is sourced through here.
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
pub extern "C" fn memalign(_alignment: c_ulong, size: c_ulong) -> *mut c_void {
    crate::alloc::osal_kmalloc(size as usize)
}

// ── Strings ──────────────────────────────────────────────────────────────────

/// `strcmp` (delegates to the OSAL implementation).
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
pub extern "C" fn strcmp(s1: *const c_char, s2: *const c_char) -> c_int {
    crate::osal_ext::osal_strcmp(s1, s2)
}

/// `strtol` (delegates to the OSAL implementation).
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
pub extern "C" fn strtol(
    cp: *const c_char,
    endp: *mut *mut c_char,
    base: core::ffi::c_uint,
) -> c_long {
    crate::osal_ext::osal_strtol(cp, endp, base)
}

/// `atoi` — base-10 `strtol`, truncated to `int`.
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
pub extern "C" fn atoi(s: *const c_char) -> c_int {
    crate::osal_ext::osal_strtol(s, core::ptr::null_mut(), 10) as c_int
}

/// `strstr` — first occurrence of `needle` in `haystack` (NULL if absent).
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
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
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
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
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
pub extern "C" fn gettimeofday(tv: *mut crate::osal_ext::OsalTimeval, _tz: *mut c_void) -> c_int {
    crate::osal_ext::osal_gettimeofday(tv);
    0
}

#[repr(C)]
pub struct Timespec {
    tv_sec: c_long,
    tv_nsec: c_long,
}

/// Monotonic `clock_gettime` used by the supplicant eloop.
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
pub extern "C" fn clock_gettime(_clock_id: c_int, ts: *mut Timespec) -> c_int {
    if ts.is_null() {
        return -1;
    }
    let us = crate::uapi::monotonic_us();
    unsafe {
        (*ts).tv_sec = (us / 1_000_000) as c_long;
        (*ts).tv_nsec = ((us % 1_000_000) * 1_000) as c_long;
    }
    0
}

/// `print_str` — emit a C string to the log sink.
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
pub extern "C" fn print_str(s: *const c_char) {
    // SAFETY: no conversions are supplied, so this call has no variadic values.
    unsafe { crate::log::osal_printk(s) };
}

/// `panic` — fatal error from the blob. Emit a marker and halt (a real handler
/// would reset; the stack here is not run on hardware yet).
#[cfg_attr(target_arch = "riscv32", unsafe(no_mangle))]
pub extern "C" fn panic() -> ! {
    crate::log_emit(b"[blob] panic\r\n");
    loop {
        core::hint::spin_loop();
    }
}
