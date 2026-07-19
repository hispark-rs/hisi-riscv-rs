//! Thin calls into the runtime selected by the application.

use core::ffi::c_void;
#[cfg(target_arch = "riscv32")]
use core::num::NonZeroU32;
use core::num::NonZeroUsize;
use hisi_rf_rtos_driver::{TaskConfig, TaskEntry, TaskPriority};

pub fn spawn(entry: TaskEntry, arg: *mut c_void, stack_size: usize) -> Option<usize> {
    spawn_with_priority(entry, arg, stack_size, 31)
}

pub fn spawn_with_priority(
    entry: TaskEntry,
    arg: *mut c_void,
    stack_size: usize,
    priority: u8,
) -> Option<usize> {
    let stack_size = NonZeroUsize::new(stack_size.max(1)).unwrap();
    let priority = TaskPriority::new(priority)?;
    hisi_rf_rtos_driver::spawn(
        entry,
        arg,
        TaskConfig {
            stack_size,
            priority,
        },
    )
    .ok()
    .map(|task| task.into_raw() as usize)
}

pub fn yield_now() {
    let _ = hisi_rf_rtos_driver::yield_now();
}

#[cfg(target_arch = "riscv32")]
pub fn sleep_ms(milliseconds: u32) {
    if let Some(milliseconds) = NonZeroU32::new(milliseconds) {
        let _ = hisi_rf_rtos_driver::sleep_ms(milliseconds);
    } else {
        yield_now();
    }
}

pub fn current_id() -> usize {
    hisi_rf_rtos_driver::current_task().map_or(usize::MAX, |task| {
        // The WS63 LiteOS ABI exposes a small scheduler slot as pid/tid. The
        // Rust runtime keeps a generation in the upper bits to reject stale
        // handles; that internal generation must not leak into the blob ABI.
        (task.into_raw() & 0xff) as usize
    })
}
