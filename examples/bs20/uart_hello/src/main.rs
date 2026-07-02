//! HiSilicon BS20 UART hello-world example.
//!
//! The BS20 analogue of `ws63-examples/uart_hello`: prints a banner and a running
//! tick counter over UART0. Built with `--features chip-bs21`, so `Uart::new_uart0`
//! drives the BS20 UART_L0 at 0x5208_1000 (vs WS63's 0x4401_0000) — the only thing
//! that changed is the base address baked into `bs2x-pac`'s `Uart0` type.
//!
//! Like the WS63 version it deliberately does NOT init the clock tree, so it
//! touches only UART0 registers and needs no CLDO_CRG/SYS_CTL modeling — ideal for
//! the milestone-1 `bs21` QEMU machine, whose UART model ignores the baud divisor.

#![no_std]
#![no_main]

use hisi_riscv_hal::Peripherals;
use hisi_riscv_hal::uart::{Config, Uart};
use hisi_riscv_rt::entry;

/// Format a u32 as decimal into `buf`, returning the used slice.
fn u32_to_dec(mut n: u32, buf: &mut [u8; 10]) -> &[u8] {
    if n == 0 {
        buf[0] = b'0';
        return &buf[..1];
    }
    let mut i = buf.len();
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    &buf[i..]
}

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let uart = Uart::new_uart0(p.UART0, Config::default());

    uart.write(b"\r\nHello from BS20 on QEMU!\r\n");
    uart.write(b"bs20-qemu: UART0 @ 0x52081000 is alive.\r\n");

    let mut tick: u32 = 0;
    loop {
        let mut buf = [0u8; 10];
        uart.write(b"tick ");
        uart.write(u32_to_dec(tick, &mut buf));
        uart.write(b"\r\n");
        tick = tick.wrapping_add(1);

        // Busy-wait between lines (~arbitrary at QEMU speed).
        for _ in 0..5_000_000 {
            core::hint::spin_loop();
        }
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
