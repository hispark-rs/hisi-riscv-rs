//! `osal_wait` — condition-variable wait objects (C SDK `kernel/osal`).
//!
//! Mirrors `osal_wait { void *wait; }` and the condition-wait API the WiFi
//! driver uses: a task sleeps in [`osal_wait_interruptible`] until a predicate
//! `func(param)` holds, woken by [`osal_wait_wakeup`] re-evaluating it. Backed
//! by a scheduler `Semaphore`: `wakeup` releases it,
//! the waiter re-checks the predicate (classic condvar recheck loop).

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::OSAL_OK;
use core::cell::RefCell;
use core::ffi::{c_int, c_uint, c_void};
use critical_section::Mutex;
use hisi_rf_rtos_driver::{Semaphore, WaitTimeout};

const OSAL_FAILURE: c_int = -1;
const WAIT_DIAGNOSTIC_SLOTS: usize = 16;

/// Low-overhead snapshot of one vendor wait object.
///
/// This is a bring-up diagnostic rather than a stable radio API. Counters are
/// updated in short critical sections; no UART output or allocation occurs in
/// wait/wakeup hot paths.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WaitDiagnostic {
    pub wait: usize,
    pub semaphore: usize,
    pub predicate: usize,
    pub parameter: usize,
    pub predicate_result: c_int,
    pub blocks: u32,
    pub wakeups: u32,
    pub ready_checks: u32,
    pub last_wait_task: usize,
    pub last_wake_task: usize,
    pub last_wait_caller: usize,
    pub last_wake_caller: usize,
}

impl WaitDiagnostic {
    const EMPTY: Self = Self {
        wait: 0,
        semaphore: 0,
        predicate: 0,
        parameter: 0,
        predicate_result: 0,
        blocks: 0,
        wakeups: 0,
        ready_checks: 0,
        last_wait_task: usize::MAX,
        last_wake_task: usize::MAX,
        last_wait_caller: 0,
        last_wake_caller: 0,
    };
}

static WAIT_DIAGNOSTICS: Mutex<RefCell<[WaitDiagnostic; WAIT_DIAGNOSTIC_SLOTS]>> =
    Mutex::new(RefCell::new([WaitDiagnostic::EMPTY; WAIT_DIAGNOSTIC_SLOTS]));

macro_rules! caller_address {
    () => {{
        #[cfg(target_arch = "riscv32")]
        {
            let value: usize;
            // SAFETY: reads the incoming return-address register before this
            // function calls anything else or modifies memory.
            unsafe {
                core::arch::asm!("mv {value}, ra", value = out(reg) value, options(nomem, nostack));
            }
            value
        }
        #[cfg(not(target_arch = "riscv32"))]
        {
            0usize
        }
    }};
}

fn update_wait_diagnostic(wait: *mut OsalWait, update: impl FnOnce(&mut WaitDiagnostic)) {
    if wait.is_null() {
        return;
    }
    let wait_address = wait as usize;
    critical_section::with(|cs| {
        let mut slots = WAIT_DIAGNOSTICS.borrow_ref_mut(cs);
        let index = slots
            .iter()
            .position(|slot| slot.wait == wait_address)
            .or_else(|| slots.iter().position(|slot| slot.wait == 0));
        if let Some(index) = index {
            if slots[index].wait == 0 {
                slots[index] = WaitDiagnostic {
                    wait: wait_address,
                    ..WaitDiagnostic::EMPTY
                };
            }
            update(&mut slots[index]);
        }
    });
}

fn record_block(
    wait: *mut OsalWait,
    predicate: WaitConditionFunc,
    parameter: *const c_void,
    caller: usize,
) {
    update_wait_diagnostic(wait, |slot| {
        slot.predicate = predicate.map_or(0, |function| function as usize);
        slot.parameter = parameter as usize;
        slot.blocks = slot.blocks.saturating_add(1);
        slot.last_wait_task = crate::runtime::current_id();
        slot.last_wait_caller = caller;
    });
}

fn record_ready(wait: *mut OsalWait, predicate: WaitConditionFunc, parameter: *const c_void) {
    update_wait_diagnostic(wait, |slot| {
        slot.predicate = predicate.map_or(0, |function| function as usize);
        slot.parameter = parameter as usize;
        slot.ready_checks = slot.ready_checks.saturating_add(1);
    });
}

/// Copies registered wait-object counters into `output` and returns the number
/// of populated entries copied.
#[doc(hidden)]
pub fn wait_diagnostics(output: &mut [WaitDiagnostic]) -> usize {
    let written = critical_section::with(|cs| {
        let slots = WAIT_DIAGNOSTICS.borrow_ref(cs);
        let mut written = 0;
        for slot in slots.iter().filter(|slot| slot.wait != 0) {
            if written == output.len() {
                break;
            }
            output[written] = *slot;
            written += 1;
        }
        written
    });
    for slot in &mut output[..written] {
        if slot.predicate != 0 {
            // SAFETY: the address was captured from a live WaitConditionFunc.
            // This explicit diagnostic is called from normal task context;
            // the vendor contract requires predicates to be read-only probes.
            let predicate: extern "C" fn(*const c_void) -> c_int =
                unsafe { core::mem::transmute(slot.predicate) };
            slot.predicate_result = predicate(slot.parameter as *const c_void);
        }
    }
    written
}

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
    update_wait_diagnostic(wait, |slot| slot.semaphore = p as usize);
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
        // SAFETY: the vendor destroy contract requires no active waiter.
        let _ = unsafe { (&*(h as *const Semaphore)).destroy() };
        crate::alloc::osal_kfree(h);
        unsafe { (*wait).wait = core::ptr::null_mut() };
    }
    critical_section::with(|cs| {
        let mut slots = WAIT_DIAGNOSTICS.borrow_ref_mut(cs);
        if let Some(slot) = slots.iter_mut().find(|slot| slot.wait == wait as usize) {
            *slot = WaitDiagnostic::EMPTY;
        }
    });
}

/// Wake a waiter so it re-evaluates its condition.
#[unsafe(no_mangle)]
pub extern "C" fn osal_wait_wakeup(wait: *mut OsalWait) {
    let caller = caller_address!();
    let s = sem_of(wait);
    if s.is_null() {
        return;
    }
    update_wait_diagnostic(wait, |slot| {
        slot.wakeups = slot.wakeups.saturating_add(1);
        slot.last_wake_task = crate::runtime::current_id();
        slot.last_wake_caller = caller;
    });
    // SAFETY: `s` points at a live Semaphore inside the wait object.
    let _ = unsafe { (*s).up() };
}

/// Block until `func(param)` is nonzero, re-checking after each wakeup. Returns
/// `OSAL_OK` once satisfied, `OSAL_NOK` on a bad handle.
#[unsafe(no_mangle)]
pub extern "C" fn osal_wait_interruptible(
    wait: *mut OsalWait,
    func: WaitConditionFunc,
    param: *const c_void,
) -> c_int {
    let caller = caller_address!();
    let s = sem_of(wait);
    if s.is_null() {
        return OSAL_FAILURE;
    }
    loop {
        if let Some(f) = func {
            if f(param) != 0 {
                record_ready(wait, func, param);
                return OSAL_OK;
            }
        } else {
            return OSAL_OK; // no predicate == immediately satisfied
        }
        record_block(wait, func, param, caller);
        // SAFETY: live Semaphore; a wakeup() releases us, then we re-check.
        if unsafe { (*s).down() }.is_err() {
            return OSAL_FAILURE;
        }
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
    let caller = caller_address!();
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
            crate::runtime::current_id(),
            wait as usize,
            u32::MAX,
            ready,
            caller,
        );
        if ready != 0 {
            record_ready(wait, func, param);
            return OSAL_OK;
        }
        record_block(wait, func, param, caller);
        // SAFETY: live Semaphore; wakeup grants a count and the condition is
        // re-evaluated after this task is scheduled again.
        if unsafe { (*s).down() }.is_err() {
            return OSAL_FAILURE;
        }
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
    let caller = caller_address!();
    let s = sem_of(wait);
    if s.is_null() {
        return OSAL_FAILURE;
    }
    #[cfg(feature = "rf-init-diag")]
    crate::rf_init_diag::trace_wait(
        b"begin",
        crate::runtime::current_id(),
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
            record_ready(wait, func, param);
            let result =
                wait_success_result(forever, deadline, crate::osal_ext::osal_get_jiffies());
            #[cfg(feature = "rf-init-diag")]
            crate::rf_init_diag::trace_wait(
                b"ready",
                crate::runtime::current_id(),
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
                    crate::runtime::current_id(),
                    wait as usize,
                    timeout_ms,
                    0,
                    caller,
                );
                return 0;
            }
            (deadline - now).min(u32::MAX as u64) as u32
        };
        record_block(wait, func, param, caller);
        // SAFETY: live Semaphore.
        if unsafe { (*s).down_timeout(WaitTimeout::from_millis(remaining)) }.is_err() {
            return OSAL_FAILURE;
        }
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
