//! Software timer service — cooperative, driven by the FRW worker loop.
//!
//! The WS63 WiFi driver uses millisecond software timers with **no tick ISR**:
//! callbacks fire synchronously when [`frw_dmac_timer_timeout_proc`] is called,
//! which [`crate::frw::frw_task_thread`] does on every iteration (parking only
//! until the nearest deadline via `next_delay_ms`). Deadlines are tracked in
//! milliseconds against the monotonic `mcycle`-derived clock
//! ([`osal_get_jiffies`](crate::osal_ext::osal_get_jiffies)).
//!
//! The OSAL adaptation timers (`osal_adapt_timer_init/mod/destroy`) register an
//! `osal_timer { void *timer; void (*handler)(unsigned long); unsigned long
//! data; unsigned int interval; }`; `handler(data)` fires when the interval
//! elapses. (`frw_dmac_create_timer` etc. are mask-ROM symbols, not ours.)

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::{OSAL_NOK, OSAL_OK};
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

// ── OSAL adaptation timers ───────────────────────────────────────────────────

/// Register a timer: stores `handler`/`data`/`interval` in `*timer` and reserves
/// a service slot (the timer does not run until [`osal_adapt_timer_mod`]).
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_timer_init(
    timer: *mut OsalTimer,
    func: *mut c_void,
    data: c_ulong,
    interval: c_uint,
) -> c_int {
    if timer.is_null() {
        return OSAL_NOK;
    }
    let slot = match alloc_slot() {
        Some(i) => i,
        None => return OSAL_NOK,
    };
    // SAFETY: caller-provided osal_timer; func is a `void (*)(unsigned long)`.
    unsafe {
        (*timer).handler = if func.is_null() {
            None
        } else {
            Some(core::mem::transmute::<*mut c_void, extern "C" fn(c_ulong)>(
                func,
            ))
        };
        (*timer).data = data;
        (*timer).interval = interval;
        (*timer).timer = (slot + 1) as *mut c_void;
    }
    with_slots(|s| {
        s[slot].timer = timer as usize;
        s[slot].interval = interval as u64;
    });
    OSAL_OK
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
pub extern "C" fn osal_adapt_timer_mod(timer: *mut OsalTimer, interval: c_uint) -> c_int {
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

/// Destroy a timer (frees its slot).
#[unsafe(no_mangle)]
pub extern "C" fn osal_adapt_timer_destroy(timer: *mut OsalTimer) -> c_int {
    let slot = match slot_of(timer) {
        Some(i) => i,
        None => return OSAL_OK,
    };
    with_slots(|s| s[slot] = EMPTY);
    // SAFETY: valid osal_timer.
    unsafe { (*timer).timer = core::ptr::null_mut() };
    OSAL_OK
}

// ── FRW timer driver (called from the worker loop) ──────────────────────────

/// Initialise the timer subsystem (clears all slots).
#[unsafe(no_mangle)]
pub extern "C" fn frw_dmac_timer_init() -> c_int {
    with_slots(|s| *s = [EMPTY; MAX_TIMERS]);
    OSAL_OK
}

/// Shut down the timer subsystem.
#[unsafe(no_mangle)]
pub extern "C" fn frw_dmac_timer_exit() -> c_int {
    with_slots(|s| *s = [EMPTY; MAX_TIMERS]);
    OSAL_OK
}

/// Fire every timer whose deadline has passed. THE driver — call each worker
/// iteration. One-shot timers deactivate after firing; periodic ones re-arm.
/// Callbacks run OUTSIDE the critical section (they may touch timers / yield).
#[unsafe(no_mangle)]
pub extern "C" fn frw_dmac_timer_timeout_proc() {
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
            f(data);
        }
    }
}

/// Generic per-timer event dispatch hook (the real dispatcher is mask-ROM).
#[unsafe(no_mangle)]
pub extern "C" fn frw_timer_timeout_proc_event(_arg: c_ulong) {}

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
