//! Bounded WS63 radio runtime compatibility shims.
//!
//! A handful of kernel and architecture primitives are referenced by the
//! delivered Wi-Fi archives. They are a bounded ABI compatibility layer over
//! native runtime contracts, not a LiteOS backend. The exact archive-level
//! surface is owned by `ws63-radio-sys`'s
//! `ws63-runtime-compat.toml`; parent CI requires this module to provide exactly
//! the symbols classified as `provided` there. The *rest* of the
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

/// `LOS_TaskLock` — suppress scheduler-driven preemption of the current task.
///
/// The C ABI cannot report installation/context failures. The native runtime
/// contract remains defensive, while the verified radio startup order installs
/// the runtime before a vendor task can reach this symbol.
#[unsafe(no_mangle)]
pub extern "C" fn LOS_TaskLock() {
    let _ = hisi_rf_rtos_driver::lock_scheduler();
}

/// `LOS_TaskUnlock` — release one scheduler-lock nesting level.
#[unsafe(no_mangle)]
pub extern "C" fn LOS_TaskUnlock() {
    let _ = hisi_rf_rtos_driver::unlock_scheduler();
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use core::num::NonZeroU32;
    use core::sync::atomic::{AtomicU32, Ordering};
    use hisi_rf_rtos_driver::{
        Error, MutexHandle, Runtime, RuntimeContract, RuntimeExecutionProfile, SemaphoreHandle,
        TaskConfig, TaskEntry, TaskId, TaskPriority, WaitCancellationOutcome, WaitOutcome,
        WaitTimeout,
    };

    struct LockRuntime {
        lock_count: AtomicU32,
        unlock_count: AtomicU32,
    }

    impl Runtime for LockRuntime {
        fn contract(&self) -> RuntimeContract {
            RuntimeContract::V1
        }

        fn execution_profile(&self) -> RuntimeExecutionProfile {
            RuntimeExecutionProfile::V1_PORTED
        }

        fn spawn(
            &self,
            _entry: TaskEntry,
            _arg: *mut c_void,
            _config: TaskConfig,
        ) -> Result<TaskId, Error> {
            Err(Error::InvalidContext)
        }

        fn yield_now(&self) -> Result<(), Error> {
            Err(Error::InvalidContext)
        }

        fn sleep_ms(&self, _milliseconds: NonZeroU32) -> Result<(), Error> {
            Err(Error::InvalidContext)
        }

        fn current_task(&self) -> Result<TaskId, Error> {
            Err(Error::InvalidContext)
        }

        fn set_task_priority(&self, _task: TaskId, _priority: TaskPriority) -> Result<(), Error> {
            Err(Error::InvalidContext)
        }

        fn cancel_wait(&self, _task: TaskId) -> Result<WaitCancellationOutcome, Error> {
            Err(Error::InvalidContext)
        }

        fn lock_scheduler(&self) -> Result<(), Error> {
            self.lock_count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        fn unlock_scheduler(&self) -> Result<(), Error> {
            self.unlock_count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        fn semaphore_create(&self, _initial: u32) -> Result<SemaphoreHandle, Error> {
            Err(Error::InvalidContext)
        }

        fn semaphore_down(
            &self,
            _semaphore: SemaphoreHandle,
            _timeout: WaitTimeout,
        ) -> Result<WaitOutcome, Error> {
            Err(Error::InvalidContext)
        }

        fn semaphore_up(&self, _semaphore: SemaphoreHandle) -> Result<(), Error> {
            Err(Error::InvalidContext)
        }

        unsafe fn semaphore_destroy(&self, _semaphore: SemaphoreHandle) -> Result<(), Error> {
            Err(Error::InvalidContext)
        }

        fn mutex_create(&self) -> Result<MutexHandle, Error> {
            Err(Error::InvalidContext)
        }

        fn mutex_lock(
            &self,
            _mutex: MutexHandle,
            _timeout: WaitTimeout,
        ) -> Result<WaitOutcome, Error> {
            Err(Error::InvalidContext)
        }

        fn mutex_unlock(&self, _mutex: MutexHandle) -> Result<(), Error> {
            Err(Error::InvalidContext)
        }

        unsafe fn mutex_destroy(&self, _mutex: MutexHandle) -> Result<(), Error> {
            Err(Error::InvalidContext)
        }
    }

    static RUNTIME: LockRuntime = LockRuntime {
        lock_count: AtomicU32::new(0),
        unlock_count: AtomicU32::new(0),
    };

    #[test]
    fn ws63_task_lock_uses_the_native_runtime_contract() {
        hisi_rf_rtos_driver::install(&RUNTIME).unwrap();

        LOS_TaskLock();
        LOS_TaskLock();
        LOS_TaskUnlock();
        LOS_TaskUnlock();

        assert_eq!(RUNTIME.lock_count.load(Ordering::Relaxed), 2);
        assert_eq!(RUNTIME.unlock_count.load(Ordering::Relaxed), 2);
    }
}
