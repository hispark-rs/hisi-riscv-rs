//! BS21 / BS2X SPI loopback example (blocking).
//!
//! The BS2X analogue of `examples/ws63/spi_loopback`. Configures SPI0 (full-
//! duplex, Mode0, 1 MHz) and round-trips a few bytes with a blocking
//! [`Spi::transfer`]. The bs2x QEMU machines (`-M bs21/bs22/bs20`) loop SPI0's
//! TX FIFO back to RX (the shared DesignWare-SSI loopback model at 0x5208_7000),
//! so the read buffer equals what was written; on real silicon, short MOSI↔MISO.
//!
//! BS2X's SPI is the same DesignWare SSI v151 IP as WS63's, so the chip-bs21 HAL
//! drives the identical register block. Unlike WS63, `new_spi0` does NOT run the
//! WS63 CLDO_CRG two-stage clock setup (that is `#[cfg(chip-ws63)]`); BS2X runs
//! the SPI off its default input clock and only programs the in-controller SCKDV
//! divider (the QEMU model ignores the divisor anyway).

#![no_std]
#![no_main]

use hisi_riscv_hal::Peripherals;
use hisi_riscv_hal::spi::{Config as SpiConfig, DataBits, Spi, SpiHz, SpiMode};
use hisi_riscv_hal::uart::{Config as UartConfig, Uart};
use hisi_riscv_rt::entry;

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let uart = Uart::new_uart0(p.UART0, UartConfig::default());
    uart.write(0, b"\r\nBS2X SPI loopback (SPI0, Mode0, 1 MHz)\r\n");

    let mut spi = Spi::new_spi0(
        p.SPI0,
        SpiConfig {
            frequency: SpiHz::ONE_MHZ,
            mode: SpiMode::Mode0,
            data_bits: DataBits::EIGHT,
        },
    );

    let tx = [0xA5u8, 0x3C, 0xFF, 0x01];
    let mut rx = [0u8; 4];
    match spi.transfer(&tx, &mut rx) {
        Ok(()) if rx == tx => uart.write(0, b"  SPI loopback OK\r\n"),
        Ok(()) => uart.write(0, b"  SPI loopback MISMATCH\r\n"),
        Err(_) => uart.write(0, b"  SPI error (timeout)\r\n"),
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
