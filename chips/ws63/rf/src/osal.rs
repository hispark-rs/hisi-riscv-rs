//! OSAL contract beyond memory/log (ws63-RF `port_osal.h`).
//!
//! Implemented for real: interrupt lock/restore (the core critical-section
//! primitive, via `mstatus.MIE`), `osal_udelay` (approximate busy-wait),
//! `osal_flush_cache` (a data `fence`). Per-line IRQ management and
//! threads/wait are typed stubs — they need the trap-delivery wiring and a task
//! scheduler that ws63-rs does not have yet (RF2/RF3 follow-up).
//!
//! `osal_kmalloc`/`osal_kfree` live in [`crate::alloc`].

use crate::OSAL_OK;
use core::ffi::{c_char, c_int, c_ulong, c_void};

/// Approximate CPU cycles per microsecond for [`osal_udelay`]. The WS63 app
/// core runs at a few hundred MHz; this is intentionally rough (the busy-wait
/// is not calibrated and QEMU is not cycle-accurate).
const CYCLES_PER_US: u64 = 240;

// ── Interrupt lock / restore (REAL) ─────────────────────────────────────────

/// Disable interrupts, returning the previous `mstatus.MIE` state for
/// [`osal_irq_restore`]. The fundamental critical-section primitive.
#[unsafe(no_mangle)]
pub extern "C" fn osal_irq_lock() -> c_ulong {
    #[cfg(target_arch = "riscv32")]
    {
        let prev: u32;
        // SAFETY: csrrci atomically reads mstatus and clears MIE (bit 3).
        unsafe {
            core::arch::asm!("csrrci {0}, mstatus, 0x8", out(reg) prev, options(nomem, nostack))
        };
        (prev & 0x8) as c_ulong
    }
    #[cfg(not(target_arch = "riscv32"))]
    0
}

/// Restore the interrupt-enable state returned by [`osal_irq_lock`].
#[unsafe(no_mangle)]
pub extern "C" fn osal_irq_restore(state: c_ulong) {
    #[cfg(target_arch = "riscv32")]
    if state & 0x8 != 0 {
        // SAFETY: re-set mstatus.MIE only if it was set before the lock.
        unsafe { core::arch::asm!("csrsi mstatus, 0x8", options(nomem, nostack)) };
    }
    #[cfg(not(target_arch = "riscv32"))]
    let _ = state;
}

// ── Delay (REAL, approximate) ───────────────────────────────────────────────

/// Busy-wait roughly `usec` microseconds. Uncalibrated (see `CYCLES_PER_US`).
#[unsafe(no_mangle)]
pub extern "C" fn osal_udelay(usec: u32) {
    let iters = (usec as u64).saturating_mul(CYCLES_PER_US);
    let mut i = 0u64;
    while i < iters {
        core::hint::spin_loop();
        i += 1;
    }
}

// ── Cache (REAL-ish) ────────────────────────────────────────────────────────

/// Data-side `fence`. WS63 is single-core with no MMU and QEMU models no cache,
/// so a memory fence is sufficient ordering for the scaffold.
#[unsafe(no_mangle)]
pub extern "C" fn osal_flush_cache(_addr: *mut c_void, _size: usize) {
    #[cfg(target_arch = "riscv32")]
    // SAFETY: plain memory fence, no operands.
    unsafe {
        core::arch::asm!("fence", options(nostack))
    };
}

// ── Per-line IRQ management (STUB — needs trap-delivery wiring, RF2/RF3) ─────

type IrqHandler = Option<unsafe extern "C" fn(u32, *mut c_void)>;

/// STUB: records nothing yet. Delivering blob IRQs needs the trap vector to
/// route the WLAN/MAC line to this handler — wired in RF2/RF3.
#[unsafe(no_mangle)]
pub extern "C" fn osal_irq_request(_irq: u32, _handler: IrqHandler, _arg: *mut c_void) -> c_int {
    OSAL_OK
}
/// STUB (see [`osal_irq_request`]).
#[unsafe(no_mangle)]
pub extern "C" fn osal_irq_free(_irq: u32, _arg: *mut c_void) -> c_int {
    OSAL_OK
}
/// STUB: per-line enable needs the local-IRQ controller wiring (RF2/RF3).
#[unsafe(no_mangle)]
pub extern "C" fn osal_irq_enable(_irq: u32) -> c_int {
    OSAL_OK
}
/// STUB (see [`osal_irq_enable`]).
#[unsafe(no_mangle)]
pub extern "C" fn osal_irq_disable(_irq: u32) -> c_int {
    OSAL_OK
}
/// Set an IRQ's priority. STUB: the WS63 local-interrupt priority model
/// (`LOCIPRI`) is owned by the runtime/HAL, not the blob — accepted as a no-op.
#[unsafe(no_mangle)]
pub extern "C" fn osal_irq_set_priority(_irq: core::ffi::c_uint, _priority: u16) -> c_int {
    OSAL_OK
}

// ── Threads (backed by the real scheduler in `crate::sched`) ────────────────

type KthreadFunc = Option<extern "C" fn(*mut c_void) -> *mut c_void>;

/// The vendor OSAL handle ABI (`osal_task { void *task; }`).
#[repr(C)]
struct OsalTask {
    task: *mut c_void,
}

/// Spawn a kernel thread on the cooperative scheduler.
///
/// The returned pointer addresses a real [`OsalTask`], matching the C SDK. The
/// Wi-Fi FRW code dereferences its first word to obtain the task id; returning
/// an integer disguised as a pointer here is therefore not a valid opaque
/// handle implementation.
#[unsafe(no_mangle)]
pub extern "C" fn osal_kthread_create(
    func: KthreadFunc,
    arg: *mut c_void,
    _name: *const c_char,
    stack_size: usize,
) -> *mut c_void {
    let Some(f) = func else {
        return core::ptr::null_mut();
    };
    let handle = crate::alloc::osal_kmalloc(core::mem::size_of::<OsalTask>()) as *mut OsalTask;
    if handle.is_null() {
        return core::ptr::null_mut();
    }
    match crate::sched::spawn(f, arg, stack_size) {
        Some(slot) => {
            // SAFETY: `handle` owns an allocation large and aligned enough for
            // `OsalTask`; the allocation remains live for the long-lived Wi-Fi
            // worker task.
            unsafe {
                handle.write(OsalTask {
                    task: slot as *mut c_void,
                });
            }
            handle.cast()
        }
        None => {
            crate::alloc::osal_kfree(handle.cast());
            core::ptr::null_mut()
        }
    }
}
/// Destroy a thread. NO-OP for now: cleanly killing an arbitrary task (freeing
/// the stack it may be running on) needs deferred reclamation — TODO. The WiFi
/// worker threads are long-lived, so this is acceptable for the scaffold.
#[unsafe(no_mangle)]
pub extern "C" fn osal_kthread_destroy(_thread: *mut c_void, _stop_flag: u32) {}
/// Prevent preemption. The scheduler is cooperative (no time-slicing yet), so a
/// task already runs to its next yield/block — this is a no-op.
#[unsafe(no_mangle)]
pub extern "C" fn osal_kthread_lock() {}
/// Re-allow preemption (see [`osal_kthread_lock`]).
#[unsafe(no_mangle)]
pub extern "C" fn osal_kthread_unlock() {}
/// Set thread priority. NO-OP: the cooperative scheduler is round-robin (no
/// priorities yet) — TODO when preemption lands.
#[unsafe(no_mangle)]
pub extern "C" fn osal_kthread_set_priority(_thread: *mut c_void, _priority: c_int) -> c_int {
    OSAL_OK
}

/// Sleep the current task for `ms` milliseconds (scheduler-backed).
#[unsafe(no_mangle)]
pub extern "C" fn osal_msleep(ms: u32) {
    crate::sched::sleep_ms(ms);
}

/// Current task id ("pid"/"tid") — the scheduler slot index.
#[unsafe(no_mangle)]
pub extern "C" fn osal_get_current_pid() -> c_int {
    crate::sched::current_id() as c_int
}
/// Current task id (alias of [`osal_get_current_pid`]).
#[unsafe(no_mangle)]
pub extern "C" fn osal_get_current_tid() -> c_int {
    crate::sched::current_id() as c_int
}

// Wait objects (`osal_wait { void *wait; }`, condition-variable semantics) live
// in [`crate::osal_wait`] — the C SDK signatures take the struct pointer.
