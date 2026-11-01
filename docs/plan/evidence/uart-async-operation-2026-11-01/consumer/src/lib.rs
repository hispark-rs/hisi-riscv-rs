#![no_std]
#[cfg(feature = "standalone")]
pub use hisi_rtos::embassy::{init, on_alarm_interrupt, InitError};
#[cfg(not(feature = "standalone"))]
pub use hisi_rtos::{start_cooperative, start_with_port, SchedulerStorage};
pub fn resolution_us() -> u32 { hisi_rtos::embassy::RESOLUTION_US }
#[test]
fn registered_provider_preserves_profile_resolution_before_initialization() {
    assert_eq!(embassy_time_driver::TICK_HZ, 1_000_000);
    assert_eq!(resolution_us(), if cfg!(feature = "standalone") { 1 } else { 1_000 });
    assert_eq!(embassy_time_driver::now(), 0);
}

#[cfg(any(feature = "chip-ws53", feature = "chip-ws63"))]
pub fn uart_trait_contracts() {
    #[cfg(feature = "chip-ws63")]
    fn both<T: embedded_io_async::Read + embedded_io_async::Write>() {}
    #[cfg(feature = "chip-ws63")]
    both::<hisi_hal::uart::Uart<'static, hisi_hal::peripherals::Uart2<'static>>>();
    #[cfg(feature = "chip-ws53")]
    {
        fn rx<T: embedded_io_async::Read>() {}
        fn tx<T: embedded_io_async::Write>() {}
        rx::<hisi_hal::uart::AsyncReceiver<'static, 'static, hisi_hal::peripherals::Uart2<'static>>>();
        tx::<hisi_hal::uart::AsyncTransmitter<'static, 'static, hisi_hal::peripherals::Uart2<'static>>>();
    }
}
