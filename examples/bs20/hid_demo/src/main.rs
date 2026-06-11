//! BS2X HID demo: KEYSCAN + QDEC (blocking).
//!
//! Exercises two BS2X-only peripherals (no WS63 analogue) in one example:
//! - KEYSCAN: start the key-matrix scanner, read one key event. The QEMU model
//!   reports a fixed key -> row 2, col 1, pressed.
//! - QDEC: enable the quadrature decoder, read the signed accumulated count. The
//!   QEMU model returns 0xFFFB -> -5 (exercises the signed decode).
//!
//! Prints both, then "HID demo OK" if they match the model's known values.

#![no_std]
#![no_main]

use hisi_riscv_hal::Peripherals;
use hisi_riscv_hal::keyscan::Keyscan;
use hisi_riscv_hal::qdec::Qdec;
use hisi_riscv_hal::uart::{Config as UartConfig, Uart};
use hisi_riscv_rt::entry;

fn put_dec_i16(uart: &Uart<'_, impl core::any::Any>, v: i16) {
    if v < 0 {
        uart.write(0, b"-");
    }
    let mut n = v.unsigned_abs() as u32;
    let mut buf = [0u8; 6];
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    uart.write(0, &buf[i..]);
}

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let uart = Uart::new_uart0(p.UART0, UartConfig::default());
    uart.write(0, b"\r\nBS2X HID demo (KEYSCAN + QDEC)\r\n");

    // KEYSCAN — read one key from a 3x3 matrix.
    let ks = Keyscan::new(p.KEYSCAN, 3, 3);
    let key = ks.read_key();
    let key_ok = match key {
        Some(k) => {
            uart.write(0, b"  key: row=");
            put_dec_i16(&uart, k.row as i16);
            uart.write(0, b" col=");
            put_dec_i16(&uart, k.col as i16);
            uart.write(
                0,
                if k.pressed {
                    b" pressed\r\n"
                } else {
                    b" released\r\n"
                },
            );
            k.row == 2 && k.col == 1 && k.pressed
        }
        None => {
            uart.write(0, b"  key: none\r\n");
            false
        }
    };

    // QDEC — read the signed accumulated count.
    let qd = Qdec::new(p.QDEC);
    let count = qd.read_count();
    uart.write(0, b"  qdec count = ");
    put_dec_i16(&uart, count);
    uart.write(0, b"\r\n");
    let qdec_ok = count == -5;

    if key_ok && qdec_ok {
        uart.write(0, b"  HID demo OK\r\n");
    } else {
        uart.write(0, b"  HID demo MISMATCH\r\n");
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
