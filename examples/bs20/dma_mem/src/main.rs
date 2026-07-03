//! BS2X DMA memory-to-memory example (blocking).
//!
//! The BS2X MDMA is the same v151 DesignWare controller as WS63's, so the
//! chip-bs21 HAL drives it via the chip-neutral register block (the mem-to-mem
//! path needs no peripheral handshake — DmaPeripheral request IDs stay WS63-only).
//! Copies a 4-word buffer src -> dst on channel 0; the QEMU PK_DMA model performs
//! the real copy on channel-enable, so dst == src and the example prints "DMA OK".

#![no_std]
#![no_main]

use hisi_riscv_hal::Peripherals;
use hisi_riscv_hal::dma::DmaDriver;
use hisi_riscv_hal::uart::{Config as UartConfig, Uart};
use hisi_riscv_hal::usb::{Speed, Usb};
use hisi_riscv_rt::entry;

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let uart = Uart::new_uart0(p.UART0, UartConfig::default());
    uart.write(b"\r\nBS2X DMA mem-to-mem (MDMA ch0)\r\n");

    #[repr(C, align(32))]
    struct Words([u32; 4]);
    static SRC: Words = Words([0xDEAD_BEEF, 0x1234_5678, 0xCAFE_BABE, 0x0BAD_F00D]);
    static mut DST: Words = Words([0; 4]);
    // SAFETY: this example is single-threaded and owns the DMA destination buffer.
    let dst: &'static mut [u32] = unsafe { &mut (*core::ptr::addr_of_mut!(DST)).0 };

    let dma = DmaDriver::new_dma(p.DMA);
    let chs = dma.split_channels().expect("DMA channels already claimed");
    let transfer = dma
        .start_mem_to_mem(chs.ch0, &SRC.0[..], dst)
        .expect("DMA start failed");
    let (_dma, _ch0, _src, dst) = transfer.wait().expect("DMA wait failed");

    let dma_ok = dst == &SRC.0[..];

    // USB — read the DWC OTG core-ID (presence check; full USB stack deferred).
    let mut usb = Usb::new(p.USB);
    let usb_ok = match usb.device_enumerate() {
        Ok(Speed::High) => {
            uart.write(b"  usb device enumerated at high speed\r\n");
            true
        }
        Ok(_) => {
            uart.write(b"  usb device enumerated (other speed)\r\n");
            true
        }
        Err(_) => {
            uart.write(b"  usb bring-up failed\r\n");
            false
        }
    };

    if dma_ok && usb_ok {
        uart.write(b"  DMA OK\r\n");
    } else {
        uart.write(b"  DMA MISMATCH\r\n");
    }

    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
