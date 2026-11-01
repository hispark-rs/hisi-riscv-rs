#![no_std]
#[cfg(feature = "chip-ws63")]
pub async fn receive<'d>(uart: &mut hisi_hal::uart::Uart<'d, hisi_hal::peripherals::Uart0<'d>>, buf: &mut [u8]) -> usize {
    embedded_io_async::Read::read(uart, buf).await.unwrap()
}
#[cfg(feature = "chip-ws53")]
pub async fn receive<'d>(uart: &mut hisi_hal::uart::AsyncReceiver<'_, 'd, hisi_hal::peripherals::Uart0<'d>>, buf: &mut [u8]) -> Result<usize, hisi_hal::uart::ReceiveError> {
    embedded_io_async::Read::read(uart, buf).await
}
#[cfg(feature = "chip-bs21")]
pub fn uart_config() -> hisi_hal::uart::Config { hisi_hal::uart::Config::default() }
