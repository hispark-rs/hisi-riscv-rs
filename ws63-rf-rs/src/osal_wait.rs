//! `osal_wait` — condition-variable wait objects (C SDK `kernel/osal`).
//!
//! Mirrors `osal_wait { void *wait; }` and the condition-wait API the WiFi
//! driver uses: a task sleeps in [`osal_wait_interruptible`] until a predicate
//! `func(param)` holds, woken by [`osal_wait_wakeup`] re-evaluating it. Backed
//! by a scheduler [`Semaphore`](crate::sched::Semaphore): `wakeup` releases it,
//! the waiter re-checks the predicate (classic condvar recheck loop).

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::sched::Semaphore;
use crate::{OSAL_NOK, OSAL_OK};
use core::ffi::{c_int, c_uint, c_void};

/// Mirrors C `osal_wait { void *wait; }`. `.wait` holds a heap [`Semaphore`].
#[repr(C)]
pub struct OsalWait {
    pub wait: *mut c_void,
}

/// C `osal_wait_condition_func`: `int (*)(const void *param)` — nonzero == ready.
pub type WaitConditionFunc = Option<extern "C" fn(*const c_void) -> c_int>;

fn sem_of(wait: *mut OsalWait) -> *const Semaphore {
    if wait.is_null() {
        return core::ptr::null();
    }
    // SAFETY: `wait` is a valid osal_wait when non-null.
    unsafe { (*wait).wait as *const Semaphore }
}

/// Initialise a wait object (allocates its backing count-0 semaphore).
#[unsafe(no_mangle)]
pub extern "C" fn osal_wait_init(wait: *mut OsalWait) -> c_int {
    if wait.is_null() {
        return OSAL_NOK;
    }
    let p = crate::alloc::osal_kmalloc(core::mem::size_of::<Semaphore>()) as *mut Semaphore;
    if p.is_null() {
        return OSAL_NOK;
    }
    // SAFETY: freshly allocated, size_of::<Semaphore>() bytes, 8-aligned.
    unsafe {
        p.write(Semaphore::new(0));
        (*wait).wait = p as *mut c_void;
    }
    OSAL_OK
}

/// Destroy a wait object (frees its semaphore).
#[unsafe(no_mangle)]
pub extern "C" fn osal_wait_destroy(wait: *mut OsalWait) -> c_int {
    if wait.is_null() {
        return OSAL_NOK;
    }
    // SAFETY: valid osal_wait.
    let h = unsafe { (*wait).wait };
    if !h.is_null() {
        crate::alloc::osal_kfree(h);
        unsafe { (*wait).wait = core::ptr::null_mut() };
    }
    OSAL_OK
}

/// Wake a waiter so it re-evaluates its condition.
#[unsafe(no_mangle)]
pub extern "C" fn osal_wait_wakeup(wait: *mut OsalWait) -> c_int {
    let s = sem_of(wait);
    if s.is_null() {
        return OSAL_NOK;
    }
    // SAFETY: `s` points at a live Semaphore inside the wait object.
    unsafe { (*s).up() };
    OSAL_OK
}

/// Block until `func(param)` is nonzero, re-checking after each wakeup. Returns
/// `OSAL_OK` once satisfied, `OSAL_NOK` on a bad handle.
#[unsafe(no_mangle)]
pub extern "C" fn osal_wait_interruptible(
    wait: *mut OsalWait,
    func: WaitConditionFunc,
    param: *const c_void,
) -> c_int {
    let s = sem_of(wait);
    if s.is_null() {
        return OSAL_NOK;
    }
    loop {
        if let Some(f) = func {
            if f(param) != 0 {
                return OSAL_OK;
            }
        } else {
            return OSAL_OK; // no predicate == immediately satisfied
        }
        // SAFETY: live Semaphore; a wakeup() releases us, then we re-check.
        unsafe { (*s).down() };
    }
}

/// Like [`osal_wait_interruptible`] but bounded by `timeout_ms` (`u32::MAX` ==
/// forever). Returns `OSAL_OK` if satisfied, `OSAL_NOK` on timeout / bad handle
/// (matches the driver's "0 == timed out" usage).
#[unsafe(no_mangle)]
pub extern "C" fn osal_wait_timeout_interruptible(
    wait: *mut OsalWait,
    func: WaitConditionFunc,
    param: *const c_void,
    timeout_ms: c_uint,
) -> c_int {
    let s = sem_of(wait);
    if s.is_null() {
        return OSAL_NOK;
    }
    let forever = timeout_ms == u32::MAX;
    let deadline = crate::osal_ext::osal_get_jiffies().wrapping_add(timeout_ms as u64);
    loop {
        if let Some(f) = func {
            if f(param) != 0 {
                return OSAL_OK;
            }
        } else {
            return OSAL_OK;
        }
        let remaining = if forever {
            u32::MAX
        } else {
            let now = crate::osal_ext::osal_get_jiffies();
            if now >= deadline {
                return OSAL_NOK;
            }
            (deadline - now).min(u32::MAX as u64) as u32
        };
        // SAFETY: live Semaphore.
        unsafe { (*s).down_timeout(remaining) };
    }
}
