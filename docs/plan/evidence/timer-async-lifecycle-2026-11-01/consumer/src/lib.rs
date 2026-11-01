#![no_std]
use hisi_hal::{peripherals::Timer, timer::{AsyncDelay, TimerChannel}};
use embedded_hal_async::delay::DelayNs;
pub fn own(timer: Timer<'static>) -> AsyncDelay<'static> {
    AsyncDelay::new(timer, TimerChannel::Channel0)
}
pub async fn wait(delay: &mut AsyncDelay<'_>) {
    delay.delay_ns(43).await;
    delay.delay_ms(u32::MAX).await;
}
