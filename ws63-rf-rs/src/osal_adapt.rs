//! `osal_adapt_*` — the WiFi-driver's thin OSAL adaptation shim
//! (`kernel/osal_adapt/inc/osal_adapt.h`). Most are 1:1 forwarders to the
//! `osal_*` / `osal_atomic_*` / `osal_event_*` primitives already implemented;
//! software timers and workqueues are stubs pending a timer service.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::osal_queue::OsalEvent;
use crate::osal_sync::OsalAtomic;
use crate::{OSAL_NOK, OSAL_OK};
use core::ffi::{c_char, c_int, c_uint, c_void};

// ── Atomics (forward to osal_atomic_*; same `osal_atomic` type) ──────────────

/// Initialise to 0.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_atomic_init(atomic: *mut OsalAtomic) -> c_int {
    crate::osal_sync::osal_atomic_set(atomic, 0);
    OSAL_OK
}
/// Destroy (no-op).
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_atomic_destroy(_atomic: *mut OsalAtomic) {}
/// Read.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_atomic_read(atomic: *mut OsalAtomic) -> c_int {
    crate::osal_sync::osal_atomic_read(atomic)
}
/// Set.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_atomic_set(atomic: *mut OsalAtomic, val: c_int) {
    crate::osal_sync::osal_atomic_set(atomic, val);
}
/// Increment.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_atomic_inc(atomic: *mut OsalAtomic) {
    crate::osal_sync::osal_atomic_inc(atomic);
}
/// Decrement.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_atomic_dec(atomic: *mut OsalAtomic) {
    crate::osal_sync::osal_atomic_dec(atomic);
}
/// Increment, return new.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_atomic_inc_return(atomic: *mut OsalAtomic) -> c_int {
    crate::osal_sync::osal_atomic_inc_return(atomic)
}
/// Decrement, return new.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_atomic_dec_return(atomic: *mut OsalAtomic) -> c_int {
    crate::osal_sync::osal_atomic_dec_return(atomic)
}
/// Add `val` (single-hart cooperative: read+set is race-free here).
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_atomic_add(atomic: *mut OsalAtomic, val: c_int) {
    let cur = crate::osal_sync::osal_atomic_read(atomic);
    crate::osal_sync::osal_atomic_set(atomic, cur.wrapping_add(val));
}

// ── Event group (forward to osal_event_*; same `osal_event` type) ────────────

/// Init.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_event_init(event_obj: *mut OsalEvent) -> c_int {
    crate::osal_queue::osal_event_init(event_obj)
}
/// Write.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_event_write(event_obj: *mut OsalEvent, mask: c_uint) -> c_int {
    crate::osal_queue::osal_event_write(event_obj, mask)
}
/// Read.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_event_read(
    event_obj: *mut OsalEvent,
    mask: c_uint,
    timeout_ms: c_uint,
    mode: c_uint,
) -> c_int {
    crate::osal_queue::osal_event_read(event_obj, mask, timeout_ms, mode)
}
/// Clear.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_event_clear(event_obj: *mut OsalEvent, mask: c_uint) -> c_int {
    crate::osal_queue::osal_event_clear(event_obj, mask)
}
/// Destroy.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_event_destroy(event_obj: *mut OsalEvent) -> c_int {
    crate::osal_queue::osal_event_destroy(event_obj)
}

// ── IRQ (forward to osal_irq_*) ──────────────────────────────────────────────

/// Disable interrupts, return prior state.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_irq_lock() -> c_uint {
    crate::osal::osal_irq_lock() as c_uint
}
/// Restore interrupts.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_irq_restore(irq_status: c_uint) {
    crate::osal::osal_irq_restore(irq_status as core::ffi::c_ulong);
}

// ── Jiffies (forward to osal_ext) ────────────────────────────────────────────

/// Monotonic jiffies (== ms).
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_get_jiffies() -> u64 {
    crate::osal_ext::osal_get_jiffies()
}

// ── Threads (create -> scheduler; rest cooperative no-ops) ───────────────────

type KthreadHandler = Option<extern "C" fn(*mut c_void) -> *mut c_void>;

/// Spawn a task; returns an opaque `osal_task*` handle (slot+1) or NULL.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_kthread_create(
    thread: KthreadHandler,
    data: *mut c_void,
    _name: *const c_char,
    stack_size: c_uint,
) -> *mut c_void {
    match thread {
        Some(f) => match crate::sched::spawn(f, data, stack_size as usize) {
            Some(slot) => (slot + 1) as *mut c_void,
            None => core::ptr::null_mut(),
        },
        None => core::ptr::null_mut(),
    }
}
/// Destroy a task (no-op — see `osal_kthread_destroy`).
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_kthread_destroy(_task: *mut c_void, _stop_flag: c_uint) {}
/// Prevent preemption (cooperative — no-op).
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_kthread_lock() {}
/// Re-allow preemption (cooperative — no-op).
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_kthread_unlock() {}
/// Whether the current task should stop (never, here).
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_kthread_should_stop() -> c_int {
    0
}
/// Set priority (no-op — round-robin scheduler).
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_kthread_set_priority(_task: *mut c_void, _priority: c_uint) -> c_int {
    OSAL_OK
}

// ── Software timers (STUB — needs a timer service; do not fire yet) ──────────

/// STUB: accepts the registration; the timer does not fire yet (TODO: a timer
/// service driving `func` after `interval` ms via the scheduler tick).
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_timer_init(
    _timer: *mut c_void,
    _func: *mut c_void,
    _data: core::ffi::c_ulong,
    _interval: c_uint,
) -> c_int {
    OSAL_OK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_timer_mod(_timer: *mut c_void, _interval: c_uint) -> c_int {
    OSAL_OK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_timer_destroy(_timer: *mut c_void) -> c_int {
    OSAL_OK
}

// ── Workqueues (STUB — needs a deferred-work service) ────────────────────────

/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_workqueue_init(_work: *mut c_void, _handler: *mut c_void) -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_workqueue_destroy(_work: *mut c_void) {}

// ── Wait objects (osal_wait { void *wait; } backed by a Semaphore) ──────────

/// Mirrors C `osal_wait { void *wait; }`.
#[repr(C)]
pub struct OsalWait {
    wait: *mut c_void,
}

/// Wake a task waiting on the object.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_wait_wakeup(wait: *mut OsalWait) {
    if wait.is_null() {
        return;
    }
    let h = unsafe { (*wait).wait } as *const crate::sched::Semaphore;
    if !h.is_null() {
        // SAFETY: `.wait` holds a Semaphore created by the wait-init path.
        unsafe { (*h).up() };
    }
}
/// Destroy a wait object.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_wait_destroy(wait: *mut OsalWait) {
    if wait.is_null() {
        return;
    }
    let h = unsafe { (*wait).wait };
    if !h.is_null() {
        crate::alloc::osal_kfree(h);
        unsafe { (*wait).wait = core::ptr::null_mut() };
    }
}
