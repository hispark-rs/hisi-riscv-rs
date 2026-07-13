//! Thin calls into the runtime selected by the application.

use core::ffi::c_void;
use core::num::{NonZeroU32, NonZeroUsize};
use hisi_rf_rtos_driver::{TaskConfig, TaskEntry};

pub fn spawn(entry: TaskEntry, arg: *mut c_void, stack_size: usize) -> Option<usize> {
    let stack_size = NonZeroUsize::new(stack_size.max(1)).unwrap();
    hisi_rf_rtos_driver::spawn(
        entry,
        arg,
        TaskConfig {
            stack_size,
            priority: 31,
        },
    )
    .ok()
    .map(|task| task.into_raw() as usize)
}

pub fn yield_now() {
    let _ = hisi_rf_rtos_driver::yield_now();
}

pub fn sleep_ms(milliseconds: u32) {
    if let Some(milliseconds) = NonZeroU32::new(milliseconds) {
        let _ = hisi_rf_rtos_driver::sleep_ms(milliseconds);
    } else {
        yield_now();
    }
}

pub fn current_id() -> usize {
    hisi_rf_rtos_driver::current_task().map_or(usize::MAX, |task| task.into_raw() as usize)
}
