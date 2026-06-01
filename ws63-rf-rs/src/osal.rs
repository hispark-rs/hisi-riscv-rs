//! OSAL contract beyond memory/log (ws63-RF `port_osal.h`).
//!
//! Implemented for real: interrupt lock/restore (the core critical-section
//! primitive, via `mstatus.MIE`), `osal_udelay` (approximate busy-wait),
//! `osal_flush_cache` (a data `fence`). Per-line IRQ management and
//! threads/wait are typed stubs — they need the trap-delivery wiring and a task
//! scheduler that ws63-rs does not have yet (ROADMAP phase 4 / phase 6).
//!
//! `osal_kmalloc`/`osal_kfree` live in [`crate::alloc`].

use crate::{OSAL_NOK, OSAL_OK};
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

/// Busy-wait roughly `usec` microseconds. Uncalibrated (see [`CYCLES_PER_US`]).
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

// ── Per-line IRQ management (STUB — needs trap-delivery wiring, phase 4) ─────

type IrqHandler = Option<unsafe extern "C" fn(u32, *mut c_void)>;

/// STUB: records nothing yet. Delivering blob IRQs needs the trap vector to
/// route the WLAN/MAC line to this handler — wired in phase 4.
#[unsafe(no_mangle)]
pub extern "C" fn osal_irq_request(_irq: u32, _handler: IrqHandler, _arg: *mut c_void) -> c_int {
    OSAL_OK
}
/// STUB (see [`osal_irq_request`]).
#[unsafe(no_mangle)]
pub extern "C" fn osal_irq_free(_irq: u32, _arg: *mut c_void) -> c_int {
    OSAL_OK
}
/// STUB: per-line enable needs the local-IRQ controller wiring (phase 4).
#[unsafe(no_mangle)]
pub extern "C" fn osal_irq_enable(_irq: u32) -> c_int {
    OSAL_OK
}
/// STUB (see [`osal_irq_enable`]).
#[unsafe(no_mangle)]
pub extern "C" fn osal_irq_disable(_irq: u32) -> c_int {
    OSAL_OK
}

// ── Threads (STUB — needs a task scheduler, phase 4/6) ──────────────────────

type KthreadFunc = Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>;

/// STUB: returns NULL (no scheduler). The Wi-Fi worker thread cannot run until
/// ws63-rs gains a task scheduler (ROADMAP phase 6 / an RTOS).
#[unsafe(no_mangle)]
pub extern "C" fn osal_kthread_create(
    _func: KthreadFunc,
    _arg: *mut c_void,
    _stack_size: usize,
    _priority: c_int,
    _name: *const c_char,
) -> *mut c_void {
    crate::log::log_event_wifi_print0(c"osal_kthread_create: no scheduler (stub)".as_ptr());
    core::ptr::null_mut()
}
/// STUB (see [`osal_kthread_create`]).
#[unsafe(no_mangle)]
pub extern "C" fn osal_kthread_destroy(_thread: *mut c_void) -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn osal_kthread_lock(_thread: *mut c_void) -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn osal_kthread_unlock(_thread: *mut c_void) -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn osal_kthread_set_priority(_thread: *mut c_void, _priority: c_int) -> c_int {
    OSAL_NOK
}

// ── Wait/signal (STUB — needs scheduler-backed wait objects, phase 4/6) ─────

/// STUB: returns NULL (no scheduler to back a wait object).
#[unsafe(no_mangle)]
pub extern "C" fn osal_wait_init() -> *mut c_void {
    core::ptr::null_mut()
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn osal_wait_destroy(_wait: *mut c_void) -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn osal_wait_wakeup(_wait: *mut c_void) -> c_int {
    OSAL_NOK
}
