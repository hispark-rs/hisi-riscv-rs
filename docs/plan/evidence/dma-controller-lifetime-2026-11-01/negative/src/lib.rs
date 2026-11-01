#![no_std]
use hisi_hal::{dma::DmaDriver, peripherals::Dma};
// Compile-only counterexample: never execute against hardware.
pub fn forget_and_reuse(token: &mut Dma<'static>, src: &'static [u32], dst: &'static mut [u32]) {
    let driver = DmaDriver::new_dma(token.reborrow());
    let channels = driver.split_channels().unwrap();
    if let Ok(transfer) = driver.start_mem_to_mem(channels.ch0, src, dst) {
        core::mem::forget(transfer);
    }
    let mut reclaimed = DmaDriver::new_dma(token.reborrow());
    reclaimed.enable_controller();
}
