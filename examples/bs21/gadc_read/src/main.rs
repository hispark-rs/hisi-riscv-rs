//! BS2X GADC read example (blocking).
//!
//! Powers up the BS2X 13-bit ADC (GADC, v153), reads channel AIN0 with a blocking
//! conversion, and prints the raw 18-bit signed sample over UART0. The bs2x QEMU
//! machines (`-M bs21/bs22/bs20`) model the GADC digital block at 0x5703_6000:
//! they report sample-done and a fixed test sample 0x12345, so the read returns
//! 0x00012345 and the example prints "GADC read OK". On real silicon AIN0 would
//! return the actual conversion.
//!
//! BS2X-only: the GADC has no WS63 analogue (WS63 uses LSADC v154). The driver
//! drives bs2x-pac's `gadc` block + the ANA/PMU power sub-blocks; see
//! hisi-riscv-hal `gadc.rs`.

#![no_std]
#![no_main]

use hisi_riscv_hal::Peripherals;
use hisi_riscv_hal::gadc::{AdcChannel, Gadc};
use hisi_riscv_hal::uart::{Config as UartConfig, Uart, UartInstance};
use hisi_riscv_rt::entry;

const EXPECTED: i32 = 0x0001_2345; // the QEMU GADC model's fixed test sample

fn write_hex(uart: &Uart<'_, impl UartInstance>, mut v: u32) {
    let mut buf = [b'0', b'x', 0, 0, 0, 0, 0, 0, 0, 0];
    for i in 0..8 {
        let nib = (v >> ((7 - i) * 4)) & 0xF;
        buf[2 + i] = if nib < 10 {
            b'0' + nib as u8
        } else {
            b'a' + (nib - 10) as u8
        };
    }
    let _ = &mut v;
    uart.write(&buf);
}

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let uart = Uart::new_uart0(p.UART0, UartConfig::default());
    uart.write(b"\r\nBS2X GADC read (AIN0)\r\n");

    let mut adc = Gadc::new(p.GADC);
    // `read` returns Err(ConversionTimeout) if the AFE never completes; on QEMU the
    // model fills a fixed sample, so it succeeds. Fall back to -1 on timeout.
    let sample = adc.read(AdcChannel::Ain0).unwrap_or(-1);

    uart.write(b"  AIN0 raw = ");
    write_hex(&uart, sample as u32);
    uart.write(b"\r\n");

    if sample == EXPECTED {
        uart.write(b"  GADC read OK\r\n");
    } else {
        uart.write(b"  GADC read MISMATCH\r\n");
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
