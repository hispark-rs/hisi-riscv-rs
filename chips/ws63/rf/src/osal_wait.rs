//! `osal_wait` — condition-variable wait objects (C SDK `kernel/osal`).
//!
//! Mirrors `osal_wait { void *wait; }` and the condition-wait API the WiFi
//! driver uses: a task sleeps in [`osal_wait_interruptible`] until a predicate
//! `func(param)` holds, woken by [`osal_wait_wakeup`] re-evaluating it. Backed
//! by a scheduler `Semaphore`: `wakeup` releases it,
//! the waiter re-checks the predicate (classic condvar recheck loop).

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::OSAL_OK;
use crate::sched::Semaphore;
use core::ffi::{c_int, c_uint, c_void};

const OSAL_FAILURE: c_int = -1;

/// Mirrors C `osal_wait { void *wait; }`. `.wait` holds a heap `Semaphore`.
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
        return OSAL_FAILURE;
    }
    let p = crate::alloc::osal_kmalloc(core::mem::size_of::<Semaphore>()) as *mut Semaphore;
    if p.is_null() {
        return OSAL_FAILURE;
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
pub extern "C" fn osal_wait_destroy(wait: *mut OsalWait) {
    if wait.is_null() {
        return;
    }
    // SAFETY: valid osal_wait.
    let h = unsafe { (*wait).wait };
    if !h.is_null() {
        crate::alloc::osal_kfree(h);
        unsafe { (*wait).wait = core::ptr::null_mut() };
    }
}

/// Wake a waiter so it re-evaluates its condition.
#[unsafe(no_mangle)]
pub extern "C" fn osal_wait_wakeup(wait: *mut OsalWait) {
    #[cfg(all(feature = "rf-init-diag", target_arch = "riscv32"))]
    let caller = {
        let value: usize;
        // SAFETY: reads the incoming return-address register without touching memory.
        unsafe {
            core::arch::asm!("mv {value}, ra", value = out(reg) value, options(nomem, nostack));
        }
        value
    };
    #[cfg(all(feature = "rf-init-diag", not(target_arch = "riscv32")))]
    let caller = 0usize;
    let s = sem_of(wait);
    if s.is_null() {
        return;
    }
    #[cfg(feature = "rf-init-diag")]
    crate::rf_init_diag::trace_wait(
        b"wake",
        crate::sched::current_id(),
        wait as usize,
        0,
        0,
        caller,
    );
    // SAFETY: `s` points at a live Semaphore inside the wait object.
    unsafe { (*s).up() };
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
        return OSAL_FAILURE;
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

/// LiteOS implements the uninterruptible wait with the same condition-loop as
/// its interruptible compatibility entry point. This exact symbol is part of
/// the mask-ROM callback ABI used by `frw_task_thread`.
#[unsafe(no_mangle)]
pub extern "C" fn osal_wait_uninterruptible(
    wait: *mut OsalWait,
    func: WaitConditionFunc,
    param: *const c_void,
) -> c_int {
    #[cfg(all(feature = "rf-init-diag", target_arch = "riscv32"))]
    let caller = {
        let value: usize;
        // SAFETY: reads the incoming return-address register without touching memory.
        unsafe {
            core::arch::asm!("mv {value}, ra", value = out(reg) value, options(nomem, nostack));
        }
        value
    };
    #[cfg(all(feature = "rf-init-diag", not(target_arch = "riscv32")))]
    let caller = 0usize;
    let s = sem_of(wait);
    if s.is_null() {
        return OSAL_FAILURE;
    }
    loop {
        let ready = match func {
            Some(f) => f(param),
            None => return OSAL_OK,
        };
        #[cfg(feature = "rf-init-diag")]
        crate::rf_init_diag::trace_wait(
            b"forever-pred",
            crate::sched::current_id(),
            wait as usize,
            u32::MAX,
            ready,
            caller,
        );
        if ready != 0 {
            return OSAL_OK;
        }
        // SAFETY: live Semaphore; wakeup grants a count and the condition is
        // re-evaluated after this task is scheduled again.
        unsafe { (*s).down() };
    }
}

/// Like [`osal_wait_interruptible`] but bounded by `timeout_ms` (`u32::MAX` ==
/// forever). This follows the LiteOS `wait_event_interruptible_timeout`
/// contract: `0` means timeout, `1` means the condition became true at the
/// deadline, a larger positive value is the remaining time, and `-1` is an
/// invalid argument.
#[unsafe(no_mangle)]
pub extern "C" fn osal_wait_timeout_interruptible(
    wait: *mut OsalWait,
    func: WaitConditionFunc,
    param: *const c_void,
    timeout_ms: c_uint,
) -> c_int {
    #[cfg(all(feature = "rf-init-diag", target_arch = "riscv32"))]
    let caller = {
        let value: usize;
        // SAFETY: reads the incoming return-address register without touching memory.
        unsafe {
            core::arch::asm!("mv {value}, ra", value = out(reg) value, options(nomem, nostack));
        }
        value
    };
    #[cfg(all(feature = "rf-init-diag", not(target_arch = "riscv32")))]
    let caller = 0usize;
    let s = sem_of(wait);
    if s.is_null() {
        return OSAL_FAILURE;
    }
    #[cfg(feature = "rf-init-diag")]
    crate::rf_init_diag::trace_wait(
        b"begin",
        crate::sched::current_id(),
        wait as usize,
        timeout_ms,
        0,
        caller,
    );
    let forever = timeout_ms == u32::MAX;
    if !forever && timeout_ms > i32::MAX as u32 {
        return OSAL_FAILURE;
    }
    let start = crate::osal_ext::osal_get_jiffies();
    let deadline = start.saturating_add(timeout_ms as u64);
    let predicate = match func {
        Some(predicate) => predicate,
        None => {
            return if forever {
                i32::MAX
            } else {
                timeout_ms as c_int
            };
        }
    };
    if timeout_ms == 0 {
        return c_int::from(predicate(param) != 0);
    }
    loop {
        if predicate(param) != 0 {
            let result =
                wait_success_result(forever, deadline, crate::osal_ext::osal_get_jiffies());
            #[cfg(feature = "rf-init-diag")]
            crate::rf_init_diag::trace_wait(
                b"ready",
                crate::sched::current_id(),
                wait as usize,
                timeout_ms,
                result,
                caller,
            );
            return result;
        }
        let remaining = if forever {
            u32::MAX
        } else {
            let now = crate::osal_ext::osal_get_jiffies();
            if now >= deadline {
                #[cfg(feature = "rf-init-diag")]
                crate::rf_init_diag::trace_wait(
                    b"timeout",
                    crate::sched::current_id(),
                    wait as usize,
                    timeout_ms,
                    0,
                    caller,
                );
                return 0;
            }
            (deadline - now).min(u32::MAX as u64) as u32
        };
        // SAFETY: live Semaphore.
        unsafe { (*s).down_timeout(remaining) };
    }
}

fn wait_success_result(forever: bool, deadline: u64, now: u64) -> c_int {
    if forever {
        return 1;
    }
    deadline.saturating_sub(now).clamp(1, i32::MAX as u64) as c_int
}

#[cfg(test)]
mod tests {
    use super::wait_success_result;

    #[test]
    fn timeout_wait_reports_remaining_time_or_one_at_deadline() {
        assert_eq!(wait_success_result(false, 120, 40), 80);
        assert_eq!(wait_success_result(false, 120, 120), 1);
        assert_eq!(wait_success_result(false, 120, 121), 1);
        assert_eq!(wait_success_result(true, 0, u64::MAX), 1);
    }
}
