//! OSAL contract beyond memory/log (ws63-RF `port_osal.h`).
//!
//! Implemented for real: interrupt lock/restore (the core critical-section
//! primitive, via `mstatus.MIE`), per-line WLAN interrupt registration through
//! the HAL/runtime dispatch table, `osal_udelay` (approximate busy-wait), and
//! `osal_flush_cache` (a data `fence`).
//!
//! `osal_kmalloc`/`osal_kfree` live in [`crate::alloc`].

use crate::OSAL_NOK;
use crate::OSAL_OK;
use core::cell::Cell;
use core::ffi::{c_char, c_int, c_ulong, c_void};
use critical_section::Mutex;
use hisi_riscv_hal::interrupt::{self, Interrupt, Priority};

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

// ── Per-line IRQ management ─────────────────────────────────────────────────

type IrqHandler = Option<unsafe extern "C" fn(u32, *mut c_void)>;

#[derive(Clone, Copy)]
struct IrqSlot {
    handler: IrqHandler,
    arg: usize,
    dispatch_count: u32,
}

impl IrqSlot {
    const EMPTY: Self = Self {
        handler: None,
        arg: 0,
        dispatch_count: 0,
    };
}

const IRQ_COUNT: usize = 73;
static IRQ_SLOTS: [Mutex<Cell<IrqSlot>>; IRQ_COUNT] =
    [const { Mutex::new(Cell::new(IrqSlot::EMPTY)) }; IRQ_COUNT];

fn radio_interrupt(irq: u32) -> Option<Interrupt> {
    Some(match irq {
        40 => Interrupt::COEX_WL_INT,
        41 => Interrupt::COEX_BT_INT,
        42 => Interrupt::COEX_WIFI_RESUME_INT,
        44 => Interrupt::WLPHY_INT,
        45 => Interrupt::WLMAC_INT,
        69 => Interrupt::MAC_MONITOR_INT,
        _ => return None,
    })
}

fn log_irq_event(event: &[u8], irq: u32) {
    let mut hex = [0u8; 8];
    for (index, byte) in hex.iter_mut().enumerate() {
        let nibble = ((irq >> ((7 - index) * 4)) & 0xf) as u8;
        *byte = if nibble < 10 {
            b'0' + nibble
        } else {
            b'a' + nibble - 10
        };
    }
    crate::log_emit(b"RFDBG_IRQ ");
    crate::log_emit(event);
    crate::log_emit(b" irq=0x");
    crate::log_emit(&hex);
    crate::log_emit(b"\r\n");
}

/// Register a vendor WLAN interrupt handler.
///
/// The five-argument ABI matches the SDK exactly. `thread_fn` and `name` are
/// accepted for compatibility; the radio stack currently requests only direct
/// top-half handlers. The handler itself remains vendor/ROM code.
#[unsafe(no_mangle)]
pub extern "C" fn osal_irq_request(
    irq: u32,
    handler: IrqHandler,
    thread_fn: IrqHandler,
    _name: *const c_char,
    arg: *mut c_void,
) -> c_int {
    if irq as usize >= IRQ_COUNT || handler.is_none() || thread_fn.is_some() {
        return OSAL_NOK;
    }
    critical_section::with(|cs| {
        IRQ_SLOTS[irq as usize].borrow(cs).set(IrqSlot {
            handler,
            arg: arg as usize,
            dispatch_count: 0,
        });
    });
    log_irq_event(b"request", irq);
    OSAL_OK
}

/// Remove a previously registered radio interrupt handler.
#[unsafe(no_mangle)]
pub extern "C" fn osal_irq_free(irq: u32, _arg: *mut c_void) -> c_int {
    if irq as usize >= IRQ_COUNT {
        return OSAL_NOK;
    }
    critical_section::with(|cs| IRQ_SLOTS[irq as usize].borrow(cs).set(IrqSlot::EMPTY));
    OSAL_OK
}

/// Enable a registered radio interrupt at the WS63 local interrupt controller.
#[unsafe(no_mangle)]
pub extern "C" fn osal_irq_enable(irq: u32) -> c_int {
    let Some(interrupt) = radio_interrupt(irq) else {
        return OSAL_NOK;
    };
    unsafe { interrupt::enable(interrupt) };
    // SAFETY: `osal_irq_request` has already installed this line's vendor
    // handler before the SDK calls `osal_irq_enable`, and the controller line
    // is now configured. This supplies the `mstatus.MIE` step that the C SDK's
    // early `int_setup()` performs before Wi-Fi initialization.
    unsafe { interrupt::enable_global() };
    log_irq_event(b"enable", irq);
    OSAL_OK
}

/// Disable a radio interrupt and clear its latched controller state.
#[unsafe(no_mangle)]
pub extern "C" fn osal_irq_disable(irq: u32) -> c_int {
    let Some(interrupt) = radio_interrupt(irq) else {
        return OSAL_NOK;
    };
    unsafe { interrupt::disable(interrupt) };
    OSAL_OK
}

/// Clear a radio interrupt's latched local-controller pending state.
#[unsafe(no_mangle)]
pub extern "C" fn osal_irq_clear(irq: u32) -> c_int {
    let Some(interrupt) = radio_interrupt(irq) else {
        return OSAL_NOK;
    };
    interrupt::clear_pending(interrupt);
    OSAL_OK
}

/// Set a radio interrupt's local-controller priority.
#[unsafe(no_mangle)]
pub extern "C" fn osal_irq_set_priority(irq: core::ffi::c_uint, priority: u16) -> c_int {
    let Some(interrupt) = radio_interrupt(irq) else {
        return OSAL_NOK;
    };
    let Some(priority) = u8::try_from(priority).ok().and_then(Priority::from_level) else {
        return OSAL_NOK;
    };
    interrupt::set_priority(interrupt, priority);
    OSAL_OK
}

fn dispatch_irq(irq: u32) {
    let slot = critical_section::with(|cs| {
        let cell = IRQ_SLOTS[irq as usize].borrow(cs);
        let slot = cell.get();
        cell.set(IrqSlot {
            dispatch_count: slot.dispatch_count.saturating_add(1),
            ..slot
        });
        slot
    });
    if let Some(handler) = slot.handler {
        unsafe { handler(irq, slot.arg as *mut c_void) };
    }
    if let Some(interrupt) = radio_interrupt(irq) {
        interrupt::clear_pending(interrupt);
    }
}

/// Return how many times a registered IRQ reached its vendor handler.
///
/// This is an on-silicon bring-up probe, not part of the public RF API.
#[cfg(feature = "rf-init-diag")]
#[doc(hidden)]
pub fn irq_dispatch_count(irq: u32) -> u32 {
    if irq as usize >= IRQ_COUNT {
        return 0;
    }
    critical_section::with(|cs| IRQ_SLOTS[irq as usize].borrow(cs).get().dispatch_count)
}

macro_rules! radio_irq_entry {
    ($name:ident, $irq:literal) => {
        #[unsafe(no_mangle)]
        extern "C" fn $name() {
            dispatch_irq($irq);
        }
    };
}

radio_irq_entry!(COEX_WL_INT, 40);
radio_irq_entry!(COEX_BT_INT, 41);
radio_irq_entry!(COEX_WIFI_RESUME_INT, 42);
radio_irq_entry!(WLPHY_INT, 44);
radio_irq_entry!(WLMAC_INT, 45);
radio_irq_entry!(MAC_MONITOR_INT, 69);

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
