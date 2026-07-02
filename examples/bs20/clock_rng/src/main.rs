//! BS2X RTC + TRNG example (blocking).
//!
//! Exercises two more BS2X peripherals whose IP differs from WS63:
//! - RTC (v150): a 64-bit counter with a coherent-read handshake. Read the count
//!   twice; the QEMU model advances it so the second read is larger.
//! - TRNG (v1): true RNG. Read two 32-bit words; the QEMU model varies them
//!   (xorshift) so they differ and are non-zero.
//!
//! Prints both, then "RTC+TRNG OK" if the counter advanced and the randoms differ.

#![no_std]
#![no_main]

use hisi_riscv_hal::Peripherals;
use hisi_riscv_hal::rtc::{Mode, Rtc};
use hisi_riscv_hal::trng::Trng;
use hisi_riscv_hal::uart::{Config as UartConfig, Uart, UartInstance};
use hisi_riscv_rt::entry;

fn put_hex32(uart: &Uart<'_, impl UartInstance>, v: u32) {
    let mut buf = [b'0', b'x', 0, 0, 0, 0, 0, 0, 0, 0];
    for i in 0..8 {
        let nib = (v >> ((7 - i) * 4)) & 0xF;
        buf[2 + i] = if nib < 10 {
            b'0' + nib as u8
        } else {
            b'a' + (nib - 10) as u8
        };
    }
    uart.write(&buf);
}

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let uart = Uart::new_uart0(p.UART0, UartConfig::default());
    uart.write(b"\r\nBS2X RTC + TRNG\r\n");

    // RTC — read the counter twice; it must advance.
    let rtc = Rtc::new(p.RTC, 0, Mode::FreeRun);
    let c1 = rtc.read_count() as u32;
    let c2 = rtc.read_count() as u32;
    uart.write(b"  rtc c1=");
    put_hex32(&uart, c1);
    uart.write(b" c2=");
    put_hex32(&uart, c2);
    uart.write(b"\r\n");
    let rtc_ok = c2 > c1;

    // TRNG — read two words; they must differ and be non-zero.
    let trng = Trng::new(p.TRNG);
    let r1 = trng.next_u32().unwrap_or(0);
    let r2 = trng.next_u32().unwrap_or(0);
    uart.write(b"  trng r1=");
    put_hex32(&uart, r1);
    uart.write(b" r2=");
    put_hex32(&uart, r2);
    uart.write(b"\r\n");
    let trng_ok = r1 != r2 && r1 != 0;

    if rtc_ok && trng_ok {
        uart.write(b"  RTC+TRNG OK\r\n");
    } else {
        uart.write(b"  RTC+TRNG MISMATCH\r\n");
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
