//! LiteOS / arch compatibility shims.
//!
//! A handful of LiteOS kernel + arch primitives are reachable from the WiFi
//! init path (the vendor blobs were built against LiteOS). On the cooperative
//! single-hart runtime they map to IRQ masking / no-ops. The *rest* of the
//! LiteOS + CMSIS-RTOS2 surface the blobs carry (`osMutex*`, `osTimer*`,
//! `LOS_Swtmr*`, `create_thread`, …) is **not** reachable from `uapi_wifi_init`
//! — those objects belong to off-path BT / alternate-OS-adapter code and are
//! deliberately not implemented here.

#![allow(clippy::not_unsafe_ptr_arg_deref)]
// C kernel symbols keep their exact (non-snake) names for `#[no_mangle]`.
#![allow(non_snake_case)]

use core::ffi::c_void;

/// `ArchIntLock` — disable interrupts, return prior state (== `osal_irq_lock`).
#[unsafe(no_mangle)]
pub extern "C" fn ArchIntLock() -> u32 {
    crate::osal::osal_irq_lock() as u32
}

/// `ArchIntRestore` — restore interrupt state.
#[unsafe(no_mangle)]
pub extern "C" fn ArchIntRestore(int_save: u32) {
    crate::osal::osal_irq_restore(int_save as core::ffi::c_ulong);
}

/// `LOS_TaskLock` — disable preemption. Cooperative scheduler: regions guarded
/// by this never yield, so it is a no-op (mutual exclusion already holds).
#[unsafe(no_mangle)]
pub extern "C" fn LOS_TaskLock() {}

/// `LOS_TaskUnlock` — re-enable preemption (no-op; see [`LOS_TaskLock`]).
#[unsafe(no_mangle)]
pub extern "C" fn LOS_TaskUnlock() {}

/// `OsGetIdleTaskId` — id of the idle task. No dedicated idle task here; 0.
#[unsafe(no_mangle)]
pub extern "C" fn OsGetIdleTaskId() -> u32 {
    0
}

/// `LOS_HistoryTaskCpuUsage` — per-task CPU usage stats. Not tracked; 0.
#[unsafe(no_mangle)]
pub extern "C" fn LOS_HistoryTaskCpuUsage(_task_id: u32, _mode: u32) -> u32 {
    0
}

/// `reg_rw_check_addr` — validate a register address before a raw r/w. We do not
/// restrict the address map; report OK (0).
#[unsafe(no_mangle)]
pub extern "C" fn reg_rw_check_addr(_addr: *mut c_void) -> u32 {
    0
}
