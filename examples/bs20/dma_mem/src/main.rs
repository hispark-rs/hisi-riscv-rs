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
use hisi_riscv_hal::dma::{DmaChannelConfig, DmaDriver};
use hisi_riscv_hal::uart::{Config as UartConfig, Uart};
use hisi_riscv_rt::entry;

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let uart = Uart::new_uart0(p.UART0, UartConfig::default());
    uart.write(0, b"\r\nBS2X DMA mem-to-mem (MDMA ch0)\r\n");

    let src: [u32; 4] = [0xDEAD_BEEF, 0x1234_5678, 0xCAFE_BABE, 0x0BAD_F00D];
    let mut dst: [u32; 4] = [0; 4];

    let mut dma = DmaDriver::new_dma(p.DMA);
    dma.enable_controller();
    let cfg = DmaChannelConfig::default(); // MemToMem, 32-bit, inc src+dst
    dma.configure_channel(
        0,
        src.as_ptr() as u32,
        dst.as_mut_ptr() as u32,
        src.len() as u16,
        &cfg,
    );
    dma.enable_channel(0);

    // Wait for transfer-complete (bit 0 of the done mask).
    let mut spins = 0u32;
    while dma.raw_interrupt_status().0 & 0x1 == 0 {
        spins += 1;
        if spins > 1_000_000 {
            break;
        }
        core::hint::spin_loop();
    }

    if dst == src {
        uart.write(0, b"  DMA OK\r\n");
    } else {
        uart.write(0, b"  DMA MISMATCH\r\n");
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
