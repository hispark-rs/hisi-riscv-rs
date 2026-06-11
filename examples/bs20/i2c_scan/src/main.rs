//! BS2X I2C bus-scan example (blocking).
//!
//! Brings up I2C0 (DesignWare SSI v151, the BS2X I2C core) as a master and probes
//! every 7-bit address 0x08..=0x77, printing each device that ACKs. The bs2x QEMU
//! machines (`-M bs21/bs22/bs20`) model I2C0 with exactly one slave at 0x50, so the
//! scan finds 0x50 and nothing else, and the example prints "I2C scan OK".
//!
//! BS2X's I2C is a Synopsys DesignWare core — a different IP from WS63's custom
//! v150 I2C. The chip-bs21 HAL drives the v151 register block (rewritten into
//! bs2x-pac); see hisi-riscv-hal `i2c_v151.rs`.

#![no_std]
#![no_main]

use hisi_riscv_hal::Peripherals;
use hisi_riscv_hal::i2c::{I2c, Speed};
use hisi_riscv_hal::uart::{Config as UartConfig, Uart};
use hisi_riscv_rt::entry;

const EXPECTED_ADDR: u8 = 0x50; // the QEMU I2C model's single slave

fn write_hex2(uart: &Uart<'_, impl core::any::Any>, v: u8) {
    let hex = |n: u8| if n < 10 { b'0' + n } else { b'a' + (n - 10) };
    uart.write(0, &[b'0', b'x', hex(v >> 4), hex(v & 0xF)]);
}

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let uart = Uart::new_uart0(p.UART0, UartConfig::default());
    uart.write(0, b"\r\nBS2X I2C bus scan (I2C0)\r\n");

    let mut i2c = I2c::new_i2c0(p.I2C0, Speed::Standard);

    let mut count = 0u32;
    let mut last = 0u8;
    for addr in 0x08u8..=0x77 {
        if i2c.probe(addr) {
            uart.write(0, b"  found device at ");
            write_hex2(&uart, addr);
            uart.write(0, b"\r\n");
            count += 1;
            last = addr;
        }
    }

    if count == 1 && last == EXPECTED_ADDR {
        uart.write(0, b"  I2C scan OK\r\n");
    } else {
        uart.write(0, b"  I2C scan MISMATCH\r\n");
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
