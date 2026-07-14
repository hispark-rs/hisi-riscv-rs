//! Scheduler-backed OSAL semaphores + mutexes.
//!
//! These are part of the *deeper* C SDK OSAL the WiFi blob uses (NOT the
//! documented `ws63-RF/port_*.h` contract). Signatures + the `{ void* }` handle
//! structs are from fbb_ws63 `kernel/osal/include/semaphore/osal_semaphore.h`
//! and `.../lock/osal_mutex.h`. Semaphores wrap a heap driver-level
//! [`Semaphore`]. Recursive mutex ownership and priority inheritance belong to
//! the installed runtime rather than this WS63 ABI shim.

// These are C-ABI entry points: the vendor blob passes valid `osal_semaphore*`
// / `osal_mutex*` handles, so the raw-pointer derefs are sound by the contract
// (each fn still null-checks). Marking every one `unsafe extern "C"` would not
// change the C symbol, so allow the lint at module scope.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::{OSAL_NOK, OSAL_OK};
use core::ffi::{c_int, c_uint, c_void};
use core::num::NonZeroUsize;
use hisi_rf_rtos_driver::{MutexHandle, Semaphore, WaitOutcome, WaitTimeout};

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

/// Allocate a heap `Semaphore` with `count`; returns its address (null on OOM).
fn new_sem(count: u32) -> *mut c_void {
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

fn mutex_handle(raw: *mut c_void) -> Option<MutexHandle> {
    let raw = NonZeroUsize::new(raw as usize)?;
    // SAFETY: OSAL publishes only handles returned by mutex_create and clears
    // the field when the matching destroy operation completes.
    Some(unsafe { MutexHandle::from_raw(raw) })
}

// ── Semaphores ──────────────────────────────────────────────────────────────

/// Create a counting semaphore with initial count `val`.
#[unsafe(no_mangle)]
pub extern "C" fn osal_sem_init(sem: *mut OsalSemaphore, val: c_int) -> c_int {
    if sem.is_null() || val < 0 {
        return OSAL_NOK;
    }
    let h = new_sem(val as u32);
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
            if s.down().is_ok() {
                OSAL_OK
            } else {
                OSAL_NOK
            }
        }
        None => OSAL_NOK,
    }
}

/// Acquire with a timeout (ms; `u32::MAX` == wait-forever). Returns `OSAL_OK`
/// if acquired, `OSAL_NOK` on timeout or a bad handle.
#[unsafe(no_mangle)]
pub extern "C" fn osal_sem_down_timeout(sem: *mut OsalSemaphore, timeout: c_uint) -> c_int {
    if sem.is_null() {
        return OSAL_NOK;
    }
    match handle(unsafe { (*sem).sem }) {
        Some(s)
            if matches!(
                s.down_timeout(WaitTimeout::from_millis(timeout)),
                Ok(WaitOutcome::Acquired)
            ) =>
        {
            OSAL_OK
        }
        _ => OSAL_NOK,
    }
}

/// Release.
#[unsafe(no_mangle)]
pub extern "C" fn osal_sem_up(sem: *mut OsalSemaphore) {
    if sem.is_null() {
        return;
    }
    if let Some(s) = handle(unsafe { (*sem).sem }) {
        let _ = s.up();
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
        // SAFETY: destroy requires all waiters/users to be quiesced.
        let _ = unsafe { (&*(h as *const Semaphore)).destroy() };
        crate::alloc::osal_kfree(h);
        unsafe { (*sem).sem = core::ptr::null_mut() };
    }
}

// ── Recursive mutexes ──────────────────────────────────────────────────────

/// Create a mutex.
#[unsafe(no_mangle)]
pub extern "C" fn osal_mutex_init(mutex: *mut OsalMutex) -> c_int {
    if mutex.is_null() {
        return OSAL_NOK;
    }
    match hisi_rf_rtos_driver::mutex_create() {
        Ok(handle) => {
            unsafe { (*mutex).mutex = handle.into_raw().get() as *mut c_void };
            OSAL_OK
        }
        Err(_) => OSAL_NOK,
    }
}

/// Lock (blocks until free).
#[unsafe(no_mangle)]
pub extern "C" fn osal_mutex_lock(mutex: *mut OsalMutex) -> c_int {
    if mutex.is_null() {
        return OSAL_NOK;
    }
    match mutex_handle(unsafe { (*mutex).mutex })
        .and_then(|handle| hisi_rf_rtos_driver::mutex_lock(handle, WaitTimeout::Forever).ok())
    {
        Some(WaitOutcome::Acquired) => OSAL_OK,
        _ => OSAL_NOK,
    }
}

/// Lock with a timeout (ms; `u32::MAX` == wait-forever). Returns `OSAL_OK` if
/// locked, `OSAL_NOK` on timeout or a bad handle.
#[unsafe(no_mangle)]
pub extern "C" fn osal_mutex_lock_timeout(mutex: *mut OsalMutex, timeout: c_uint) -> c_int {
    if mutex.is_null() {
        return OSAL_NOK;
    }
    match mutex_handle(unsafe { (*mutex).mutex }).and_then(|handle| {
        hisi_rf_rtos_driver::mutex_lock(handle, WaitTimeout::from_millis(timeout)).ok()
    }) {
        Some(WaitOutcome::Acquired) => OSAL_OK,
        _ => OSAL_NOK,
    }
}

/// Unlock.
#[unsafe(no_mangle)]
pub extern "C" fn osal_mutex_unlock(mutex: *mut OsalMutex) {
    if mutex.is_null() {
        return;
    }
    if let Some(handle) = mutex_handle(unsafe { (*mutex).mutex }) {
        let _ = hisi_rf_rtos_driver::mutex_unlock(handle);
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
        let handle = mutex_handle(h).expect("non-null mutex handle");
        // SAFETY: the OSAL destroy contract requires no owner or waiter.
        if unsafe { hisi_rf_rtos_driver::mutex_destroy(handle) }.is_ok() {
            unsafe { (*mutex).mutex = core::ptr::null_mut() };
        }
    }
}

// ── Spinlocks (single hart → interrupt masking) ─────────────────────────────

/// Mirrors C `osal_spinlock { void *lock; }`. On a single hart a spinlock is
/// just interrupt masking; the saved IRQ state is stashed in the handle field.
#[repr(C)]
pub struct OsalSpinlock {
    lock: *mut c_void,
}

/// Initialise a spinlock.
#[unsafe(no_mangle)]
pub extern "C" fn osal_spin_lock_init(lock: *mut OsalSpinlock) -> c_int {
    if lock.is_null() {
        return OSAL_NOK;
    }
    unsafe { (*lock).lock = core::ptr::null_mut() };
    OSAL_OK
}
/// Lock (disable interrupts; save prior state in the handle).
#[unsafe(no_mangle)]
pub extern "C" fn osal_spin_lock(lock: *mut OsalSpinlock) {
    #[cfg(all(feature = "rf-queue-guard", target_arch = "riscv32"))]
    {
        let caller: u32;
        unsafe {
            core::arch::asm!("mv {caller}, ra", caller = out(reg) caller, options(nomem, nostack));
        }
        crate::osal::set_frw_queue_guard_caller(caller);
    }
    let st = crate::osal::osal_irq_lock();
    if !lock.is_null() {
        unsafe { (*lock).lock = st as *mut c_void };
    }
}
/// Unlock (restore the interrupt state saved by [`osal_spin_lock`]).
#[unsafe(no_mangle)]
pub extern "C" fn osal_spin_unlock(lock: *mut OsalSpinlock) {
    let st = if lock.is_null() {
        0
    } else {
        unsafe { (*lock).lock as core::ffi::c_ulong }
    };
    crate::osal::osal_irq_restore(st);
}
/// Lock, saving the interrupt state into `flags`.
#[unsafe(no_mangle)]
pub extern "C" fn osal_spin_lock_irqsave(_lock: *mut OsalSpinlock, flags: *mut core::ffi::c_ulong) {
    #[cfg(all(feature = "rf-queue-guard", target_arch = "riscv32"))]
    {
        let caller: u32;
        unsafe {
            core::arch::asm!("mv {caller}, ra", caller = out(reg) caller, options(nomem, nostack));
        }
        crate::osal::set_frw_queue_guard_caller(caller);
    }
    let st = crate::osal::osal_irq_lock();
    if !flags.is_null() {
        unsafe { *flags = st };
    }
}
/// Unlock, restoring the interrupt state from `flags`.
#[unsafe(no_mangle)]
pub extern "C" fn osal_spin_unlock_irqrestore(
    _lock: *mut OsalSpinlock,
    flags: *mut core::ffi::c_ulong,
) {
    let st = if flags.is_null() {
        0
    } else {
        unsafe { *flags }
    };
    crate::osal::osal_irq_restore(st);
}
/// Lock against bottom halves (== plain lock on bare metal).
#[unsafe(no_mangle)]
pub extern "C" fn osal_spin_lock_bh(lock: *mut OsalSpinlock) {
    osal_spin_lock(lock);
}
/// Unlock against bottom halves.
#[unsafe(no_mangle)]
pub extern "C" fn osal_spin_unlock_bh(lock: *mut OsalSpinlock) {
    osal_spin_unlock(lock);
}
/// Destroy a spinlock (no-op).
#[unsafe(no_mangle)]
pub extern "C" fn osal_spin_lock_destroy(_lock: *mut OsalSpinlock) {}

// ── Atomics (osal_atomic { volatile int counter; }) ─────────────────────────

/// Mirrors C `osal_atomic { volatile int counter; }`.
#[repr(C)]
pub struct OsalAtomic {
    counter: c_int,
}

#[inline]
fn atomic_rmw<R>(a: *mut OsalAtomic, f: impl FnOnce(&mut c_int) -> R) -> R {
    // Single hart: a critical section makes the read-modify-write atomic.
    critical_section::with(|_cs| {
        // SAFETY: exclusive under the critical section; `a` is a valid handle.
        let c = unsafe { &mut (*a).counter };
        f(c)
    })
}

/// Read the atomic.
#[unsafe(no_mangle)]
pub extern "C" fn osal_atomic_read(atomic: *mut OsalAtomic) -> c_int {
    if atomic.is_null() {
        return 0;
    }
    atomic_rmw(atomic, |c| *c)
}
/// Set the atomic.
#[unsafe(no_mangle)]
pub extern "C" fn osal_atomic_set(atomic: *mut OsalAtomic, i: c_int) {
    if !atomic.is_null() {
        atomic_rmw(atomic, |c| *c = i);
    }
}
/// Increment.
#[unsafe(no_mangle)]
pub extern "C" fn osal_atomic_inc(atomic: *mut OsalAtomic) {
    if !atomic.is_null() {
        atomic_rmw(atomic, |c| *c = c.wrapping_add(1));
    }
}
/// Decrement.
#[unsafe(no_mangle)]
pub extern "C" fn osal_atomic_dec(atomic: *mut OsalAtomic) {
    if !atomic.is_null() {
        atomic_rmw(atomic, |c| *c = c.wrapping_sub(1));
    }
}
/// Increment and return the new value.
#[unsafe(no_mangle)]
pub extern "C" fn osal_atomic_inc_return(atomic: *mut OsalAtomic) -> c_int {
    if atomic.is_null() {
        return 0;
    }
    atomic_rmw(atomic, |c| {
        *c = c.wrapping_add(1);
        *c
    })
}
/// Decrement and return the new value.
#[unsafe(no_mangle)]
pub extern "C" fn osal_atomic_dec_return(atomic: *mut OsalAtomic) -> c_int {
    if atomic.is_null() {
        return 0;
    }
    atomic_rmw(atomic, |c| {
        *c = c.wrapping_sub(1);
        *c
    })
}
