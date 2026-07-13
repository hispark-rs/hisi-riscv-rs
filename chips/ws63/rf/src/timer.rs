//! Software timer service — cooperative, driven by the FRW worker loop.
//!
//! The WS63 WiFi driver uses millisecond software timers with **no tick ISR**:
//! callbacks fire synchronously when [`local_timer_timeout_proc`] is called,
//! which the crate-local FRW self-test worker does on every iteration (parking only
//! until the nearest deadline via `next_delay_ms`). Deadlines are tracked in
//! milliseconds against the mask-ROM systick clock
//! ([`osal_get_jiffies`](crate::osal_ext::osal_get_jiffies)).
//!
//! The OSAL adaptation timers (`osal_adapt_timer_init/mod/destroy`) register an
//! `osal_timer { void *timer; void (*handler)(unsigned long); unsigned long
//! data; unsigned int interval; }`; `handler(data)` fires when the interval
//! elapses. Production `frw_dmac_timer_*` entry points are mask-ROM symbols;
//! these local helpers exist only for the standalone Rust self-tests.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::{OSAL_NOK, OSAL_OK};
#[cfg(target_arch = "riscv32")]
use core::cell::Cell;
use core::cell::UnsafeCell;
use core::ffi::{c_int, c_uint, c_ulong, c_void};
use critical_section as cs;

/// Mirrors C `osal_timer`. `handler(data)` is invoked when the timer expires.
#[repr(C)]
pub struct OsalTimer {
    /// Opaque handle — we stash `slot + 1` here so the service can find the slot.
    pub timer: *mut c_void,
    /// Callback, invoked with `data`.
    pub handler: Option<extern "C" fn(c_ulong)>,
    /// Argument passed to `handler`.
    pub data: c_ulong,
    /// Interval in milliseconds.
    pub interval: c_uint,
}

const MAX_TIMERS: usize = 32;

#[derive(Clone, Copy)]
struct Slot {
    used: bool,
    active: bool,
    periodic: bool,
    timer: usize,  // *mut OsalTimer as usize (handler/data/interval live there)
    deadline: u64, // ms (jiffies)
    interval: u64, // ms
}
const EMPTY: Slot = Slot {
    used: false,
    active: false,
    periodic: false,
    timer: 0,
    deadline: 0,
    interval: 0,
};

struct Timers(UnsafeCell<[Slot; MAX_TIMERS]>);
// SAFETY: only accessed inside `cs::with` on a single hart.
unsafe impl Sync for Timers {}
static TIMERS: Timers = Timers(UnsafeCell::new([EMPTY; MAX_TIMERS]));
#[cfg(target_arch = "riscv32")]
static TIMER_WORKER_STARTED: cs::Mutex<Cell<bool>> = cs::Mutex::new(Cell::new(false));

fn now_ms() -> u64 {
    crate::osal_ext::osal_get_jiffies()
}

#[inline]
fn with_slots<R>(f: impl FnOnce(&mut [Slot; MAX_TIMERS]) -> R) -> R {
    cs::with(|_| f(unsafe { &mut *TIMERS.0.get() }))
}

fn alloc_slot() -> Option<usize> {
    with_slots(|s| {
        for (i, slot) in s.iter_mut().enumerate() {
            if !slot.used {
                *slot = EMPTY;
                slot.used = true;
                return Some(i);
            }
        }
        None
    })
}

#[cfg(target_arch = "riscv32")]
extern "C" fn timer_worker(_arg: *mut c_void) -> *mut c_void {
    loop {
        crate::runtime::sleep_ms(1);
        local_timer_timeout_proc();
    }
}

fn ensure_timer_worker() -> bool {
    #[cfg(not(target_arch = "riscv32"))]
    return true;

    #[cfg(target_arch = "riscv32")]
    {
        let should_start = cs::with(|token| {
            let started = TIMER_WORKER_STARTED.borrow(token);
            if started.get() {
                false
            } else {
                started.set(true);
                true
            }
        });
        if !should_start {
            return true;
        }
        // LiteOS creates Swt_Task at LOS_TASK_PRIORITY_HIGHEST. Radio protocol
        // deadlines depend on timer callbacks running ahead of ordinary FRW,
        // WPA and application work once the timer task becomes ready.
        if crate::runtime::spawn_with_priority(timer_worker, core::ptr::null_mut(), 4096, 0)
            .is_some()
        {
            true
        } else {
            cs::with(|token| TIMER_WORKER_STARTED.borrow(token).set(false));
            false
        }
    }
}

// ── OSAL adaptation timers ───────────────────────────────────────────────────

/// Register a fully populated OSAL timer and reserve a service slot. The timer
/// does not run until [`osal_timer_mod`].
#[unsafe(no_mangle)]
pub extern "C" fn osal_timer_init(timer: *mut OsalTimer) -> c_int {
    if timer.is_null() {
        return OSAL_NOK;
    }
    #[cfg(feature = "rf-init-diag")]
    unsafe {
        crate::rf_init_diag::trace_timer(
            b"init",
            timer as usize,
            (*timer).handler.map_or(0, |handler| handler as usize),
            (*timer).data as usize,
            (*timer).interval,
        );
    }
    // SAFETY: caller supplied an `osal_timer` matching the C layout.
    if unsafe { (*timer).handler.is_none() || !(*timer).timer.is_null() || (*timer).interval == 0 }
    {
        return OSAL_NOK;
    }
    if !ensure_timer_worker() {
        return OSAL_NOK;
    }
    let slot = match alloc_slot() {
        Some(i) => i,
        None => return OSAL_NOK,
    };
    // SAFETY: caller-provided, validated `osal_timer`.
    unsafe {
        (*timer).timer = (slot + 1) as *mut c_void;
    }
    with_slots(|s| {
        s[slot].timer = timer as usize;
        // SAFETY: the timer remains caller-owned for the registered lifetime.
        s[slot].interval = unsafe { (*timer).interval as u64 };
    });
    OSAL_OK
}

/// Adapt-layer constructor used by vendor code that passes timer fields as
/// separate arguments before entering the canonical OSAL API.
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_timer_init(
    timer: *mut OsalTimer,
    func: *mut c_void,
    data: c_ulong,
    interval: c_uint,
) -> c_int {
    if timer.is_null() || func.is_null() {
        return OSAL_NOK;
    }
    // SAFETY: `func` is the C `void (*)(unsigned long)` supplied by the blob.
    unsafe {
        (*timer).timer = core::ptr::null_mut();
        (*timer).handler = Some(core::mem::transmute::<*mut c_void, extern "C" fn(c_ulong)>(
            func,
        ));
        (*timer).data = data;
        (*timer).interval = interval;
    }
    osal_timer_init(timer)
}

fn slot_of(timer: *mut OsalTimer) -> Option<usize> {
    if timer.is_null() {
        return None;
    }
    // SAFETY: valid osal_timer.
    let h = unsafe { (*timer).timer } as usize;
    if h == 0 || h > MAX_TIMERS {
        None
    } else {
        Some(h - 1)
    }
}

/// (Re)arm a timer: it fires once after `interval` ms from now.
#[unsafe(no_mangle)]
pub extern "C" fn osal_timer_mod(timer: *mut OsalTimer, interval: c_uint) -> c_int {
    if interval == 0 {
        return OSAL_NOK;
    }
    let slot = match slot_of(timer) {
        Some(i) => i,
        None => return OSAL_NOK,
    };
    // SAFETY: valid osal_timer.
    unsafe { (*timer).interval = interval };
    let now = now_ms();
    with_slots(|s| {
        if !s[slot].used {
            return OSAL_NOK;
        }
        s[slot].interval = interval as u64;
        s[slot].deadline = now + interval as u64;
        s[slot].active = true;
        s[slot].periodic = false;
        OSAL_OK
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_timer_mod(timer: *mut OsalTimer, interval: c_uint) -> c_int {
    osal_timer_mod(timer, interval)
}

/// Stop a timer. Returns `1` when an active timer was stopped and `0` when it
/// was already inactive, matching the LiteOS OSAL contract.
#[unsafe(no_mangle)]
pub extern "C" fn osal_timer_stop(timer: *mut OsalTimer) -> c_int {
    let slot = match slot_of(timer) {
        Some(i) => i,
        None => return OSAL_NOK,
    };
    with_slots(|s| {
        let was_active = s[slot].active;
        s[slot].active = false;
        i32::from(was_active)
    })
}

/// Destroy a timer (frees its slot).
#[unsafe(no_mangle)]
pub extern "C" fn osal_timer_destroy(timer: *mut OsalTimer) -> c_int {
    let slot = match slot_of(timer) {
        Some(i) => i,
        None => return OSAL_OK,
    };
    with_slots(|s| s[slot] = EMPTY);
    // SAFETY: valid osal_timer.
    unsafe { (*timer).timer = core::ptr::null_mut() };
    OSAL_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_timer_destroy(timer: *mut OsalTimer) -> c_int {
    osal_timer_destroy(timer)
}

// ── FRW timer driver (called from the worker loop) ──────────────────────────

/// Initialise the timer subsystem (clears all slots).
pub(crate) fn local_timer_init() -> c_int {
    with_slots(|s| *s = [EMPTY; MAX_TIMERS]);
    OSAL_OK
}

/// Shut down the timer subsystem.
#[allow(dead_code)]
pub(crate) fn local_timer_exit() -> c_int {
    with_slots(|s| *s = [EMPTY; MAX_TIMERS]);
    OSAL_OK
}

/// Fire every timer whose deadline has passed. THE driver — call each worker
/// iteration. One-shot timers deactivate after firing; periodic ones re-arm.
/// Callbacks run OUTSIDE the critical section (they may touch timers / yield).
pub(crate) fn local_timer_timeout_proc() {
    let now = now_ms();
    // Collect due timers under the lock, then fire them unlocked.
    let mut due: [(usize, u64); MAX_TIMERS] = [(usize::MAX, 0); MAX_TIMERS];
    let n = with_slots(|s| {
        let mut k = 0;
        for slot in s.iter_mut() {
            if slot.used && slot.active && now >= slot.deadline {
                due[k] = (slot.timer, 0);
                k += 1;
                if slot.periodic {
                    slot.deadline = now + slot.interval;
                } else {
                    slot.active = false;
                }
            }
        }
        k
    });
    for &(timer_addr, _) in due.iter().take(n) {
        let t = timer_addr as *mut OsalTimer;
        if t.is_null() {
            continue;
        }
        // SAFETY: the slot held a live osal_timer pointer.
        let (handler, data) = unsafe { ((*t).handler, (*t).data) };
        if let Some(f) = handler {
            #[cfg(feature = "rf-init-diag")]
            unsafe {
                crate::rf_init_diag::trace_timer(
                    b"fire",
                    t as usize,
                    f as usize,
                    data as usize,
                    (*t).interval,
                );
            }
            f(data);
        }
    }
}

/// Generic per-timer event dispatch hook (the real dispatcher is mask-ROM).
#[allow(dead_code)]
pub(crate) fn local_timer_timeout_proc_event(_arg: c_ulong) {}

// ── Worker integration ───────────────────────────────────────────────────────

/// Milliseconds until the nearest active timer's deadline (clamped to
/// `[1, cap]`), or `u32::MAX` if no timer is armed (the worker then blocks until
/// a message arrives). Used by the FRW worker to bound its park.
pub(crate) fn next_delay_ms() -> u32 {
    let now = now_ms();
    with_slots(|s| {
        let mut best: Option<u64> = None;
        for slot in s.iter() {
            if slot.used && slot.active {
                let d = slot.deadline.saturating_sub(now);
                best = Some(best.map_or(d, |b| b.min(d)));
            }
        }
        match best {
            None => u32::MAX,
            Some(d) => d.clamp(1, u32::MAX as u64) as u32,
        }
    })
}
