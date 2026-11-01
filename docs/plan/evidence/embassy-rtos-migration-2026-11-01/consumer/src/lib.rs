#![no_std]
pub use hisi_rtos::embassy::{InitError, Status, init, on_alarm_interrupt};
pub fn resolution_us() -> u32 { hisi_rtos::embassy::RESOLUTION_US }
#[test]
fn uninitialized_time_is_safe_and_has_one_registered_provider() {
    assert_eq!(resolution_us(), 1);
    assert_eq!(embassy_time_driver::now(), 0);
    assert!(!hisi_rtos::embassy::status().initialized);
}
