//! Internal self-test for the netif→smoltcp bridge (feature `net`).
//!
//! Stands up a smoltcp `Interface` over the `Ws63Device`, injects an ARP request
//! for our IP through the RX seam (as `driverif_input` would), polls, and
//! confirms smoltcp transmits the matching ARP reply through the TX seam — i.e.
//! a frame round-trips driver→smoltcp→driver without the vendor blob. Reports
//! over UART0; prints `NET SELFTEST: PASS` on `ws63-qemu`.
//!
//! Run: `cargo build -p ws63-rf-rs --example net_selftest --features net --release`

#![no_std]
#![no_main]

use hisi_riscv_hal::Peripherals;
use hisi_riscv_hal::uart::{Config, Uart};
use hisi_riscv_rt::entry;

fn u32dec(mut v: u32, buf: &mut [u8; 10]) -> &[u8] {
    let mut i = buf.len();
    if v == 0 {
        i -= 1;
        buf[i] = b'0';
    }
    while v > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    &buf[i..]
}

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let uart = Uart::new_uart0(p.UART0, Config::default());
    uart.write(
        0,
        b"\r\nWS63 ws63-rf-rs netif->smoltcp bridge self-test\r\n",
    );

    let r = ws63_rf_rs::netif_smoltcp_selftest(); // [tx_count, reply_ok, ok]
    let labels: [&[u8]; 3] = [
        b"arp tx_count    = ",
        b"arp reply_ok    = ",
        b"bridge ok       = ",
    ];
    let mut b = [0u8; 10];
    for (label, v) in labels.iter().zip(r.iter()) {
        uart.write(0, label);
        uart.write(0, u32dec(*v, &mut b));
        uart.write(0, b"\r\n");
    }

    uart.write(
        0,
        if r == [1, 1, 1] {
            b"NET SELFTEST: PASS\r\n"
        } else {
            b"NET SELFTEST: FAIL\r\n"
        },
    );

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
