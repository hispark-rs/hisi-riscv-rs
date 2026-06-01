//! Scheduler-backed OSAL semaphores + mutexes.
//!
//! These are part of the *deeper* C SDK OSAL the WiFi blob uses (NOT the
//! documented `ws63-RF/port_*.h` contract). Signatures + the `{ void* }` handle
//! structs are from fbb_ws63 `kernel/osal/include/semaphore/osal_semaphore.h`
//! and `.../lock/osal_mutex.h`. Each wraps a heap [`crate::sched::Semaphore`]
//! (a mutex is a binary semaphore initialised available).

// These are C-ABI entry points: the vendor blob passes valid `osal_semaphore*`
// / `osal_mutex*` handles, so the raw-pointer derefs are sound by the contract
// (each fn still null-checks). Marking every one `unsafe extern "C"` would not
// change the C symbol, so allow the lint at module scope.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::sched::Semaphore;
use crate::{OSAL_NOK, OSAL_OK};
use core::ffi::{c_int, c_uint, c_void};

/// Mirrors C `osal_semaphore { void *sem; }` (caller-provided).
#[repr(C)]
pub struct OsalSemaphore {
    sem: *mut c_void,
}
/// Mirrors C `osal_mutex { void *mutex; }` (caller-provided).
#[repr(C)]
pub struct OsalMutex {
    mutex: *mut c_void,
}

/// Allocate a heap [`Semaphore`] with `count`; returns its address (null on OOM).
fn new_sem(count: i32) -> *mut c_void {
    let p = crate::alloc::osal_kmalloc(core::mem::size_of::<Semaphore>()) as *mut Semaphore;
    if !p.is_null() {
        // SAFETY: freshly allocated, sized + 8-aligned for Semaphore.
        unsafe { p.write(Semaphore::new(count)) };
    }
    p as *mut c_void
}

/// Borrow the heap `Semaphore` behind a handle field, if set.
fn handle<'a>(h: *mut c_void) -> Option<&'a Semaphore> {
    let h = h as *const Semaphore;
    if h.is_null() {
        None
    } else {
        // SAFETY: `h` came from `new_sem` and lives until destroy.
        Some(unsafe { &*h })
    }
}

// ── Semaphores ──────────────────────────────────────────────────────────────

/// Create a counting semaphore with initial count `val`.
#[unsafe(no_mangle)]
pub extern "C" fn osal_sem_init(sem: *mut OsalSemaphore, val: c_int) -> c_int {
    if sem.is_null() {
        return OSAL_NOK;
    }
    let h = new_sem(val);
    if h.is_null() {
        return OSAL_NOK;
    }
    unsafe { (*sem).sem = h };
    OSAL_OK
}

/// Create a binary semaphore (count clamped to 0/1).
#[unsafe(no_mangle)]
pub extern "C" fn osal_sem_binary_sem_init(sem: *mut OsalSemaphore, val: c_int) -> c_int {
    osal_sem_init(sem, if val != 0 { 1 } else { 0 })
}

/// Acquire (blocks until available).
#[unsafe(no_mangle)]
pub extern "C" fn osal_sem_down(sem: *mut OsalSemaphore) -> c_int {
    if sem.is_null() {
        return OSAL_NOK;
    }
    match handle(unsafe { (*sem).sem }) {
        Some(s) => {
            s.down();
            OSAL_OK
        }
        None => OSAL_NOK,
    }
}

/// Acquire with a timeout. TODO: the timeout is not yet honored (blocks until
/// available); a timed block needs the sleep/timer path wired into `down`.
#[unsafe(no_mangle)]
pub extern "C" fn osal_sem_down_timeout(sem: *mut OsalSemaphore, _timeout: c_uint) -> c_int {
    osal_sem_down(sem)
}

/// Release.
#[unsafe(no_mangle)]
pub extern "C" fn osal_sem_up(sem: *mut OsalSemaphore) {
    if sem.is_null() {
        return;
    }
    if let Some(s) = handle(unsafe { (*sem).sem }) {
        s.up();
    }
}

/// Destroy a semaphore.
#[unsafe(no_mangle)]
pub extern "C" fn osal_sem_destroy(sem: *mut OsalSemaphore) {
    if sem.is_null() {
        return;
    }
    let h = unsafe { (*sem).sem };
    if !h.is_null() {
        crate::alloc::osal_kfree(h);
        unsafe { (*sem).sem = core::ptr::null_mut() };
    }
}

// ── Mutexes (binary semaphore, initialised available) ───────────────────────

/// Create a mutex.
#[unsafe(no_mangle)]
pub extern "C" fn osal_mutex_init(mutex: *mut OsalMutex) -> c_int {
    if mutex.is_null() {
        return OSAL_NOK;
    }
    let h = new_sem(1);
    if h.is_null() {
        return OSAL_NOK;
    }
    unsafe { (*mutex).mutex = h };
    OSAL_OK
}

/// Lock (blocks until free).
#[unsafe(no_mangle)]
pub extern "C" fn osal_mutex_lock(mutex: *mut OsalMutex) -> c_int {
    if mutex.is_null() {
        return OSAL_NOK;
    }
    match handle(unsafe { (*mutex).mutex }) {
        Some(s) => {
            s.down();
            OSAL_OK
        }
        None => OSAL_NOK,
    }
}

/// Lock with a timeout (timeout not yet honored — see [`osal_sem_down_timeout`]).
#[unsafe(no_mangle)]
pub extern "C" fn osal_mutex_lock_timeout(mutex: *mut OsalMutex, _timeout: c_uint) -> c_int {
    osal_mutex_lock(mutex)
}

/// Unlock.
#[unsafe(no_mangle)]
pub extern "C" fn osal_mutex_unlock(mutex: *mut OsalMutex) {
    if mutex.is_null() {
        return;
    }
    if let Some(s) = handle(unsafe { (*mutex).mutex }) {
        s.up();
    }
}

/// Destroy a mutex.
#[unsafe(no_mangle)]
pub extern "C" fn osal_mutex_destroy(mutex: *mut OsalMutex) {
    if mutex.is_null() {
        return;
    }
    let h = unsafe { (*mutex).mutex };
    if !h.is_null() {
        crate::alloc::osal_kfree(h);
        unsafe { (*mutex).mutex = core::ptr::null_mut() };
    }
}
