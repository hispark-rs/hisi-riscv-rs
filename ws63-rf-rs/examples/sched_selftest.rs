//! Internal self-test for the ws63-rf-rs cooperative scheduler.
//!
//! Self-contained with the crate (the scheduler is internal runtime plumbing,
//! not a public API). Runs [`ws63_rf_rs::sched_selftest`] — two worker tasks
//! (context switching) + a producer/consumer semaphore handoff (park/wake) —
//! and reports over UART0. Prints `SCHED SELFTEST: PASS` on `ws63-qemu`.
//!
//! Run: `cargo build -p ws63-rf-rs --example sched_selftest --release`

#![no_std]
#![no_main]

use ws63_hal::Peripherals;
use ws63_hal::uart::{Config, Uart};
use ws63_rt::entry;

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
    uart.write(0, b"\r\nWS63 ws63-rf-rs scheduler self-test\r\n");

    let r = ws63_rf_rs::sched_selftest(); // [worker0, worker1, sem_items, done]

    let labels: [&[u8]; 4] = [
        b"worker0 counter = ",
        b"worker1 counter = ",
        b"semaphore items = ",
        b"tasks finished  = ",
    ];
    let mut b = [0u8; 10];
    for (label, v) in labels.iter().zip(r.iter()) {
        uart.write(0, label);
        uart.write(0, u32dec(*v, &mut b));
        uart.write(0, b"\r\n");
    }

    // Also exercise the scheduler-backed OSAL message queue (write -> read).
    let q = ws63_rf_rs::osal_queue_selftest();
    uart.write(0, b"osal_msg_queue rx= 0x");
    {
        let mut hb = [0u8; 8];
        let mut i = 0;
        while i < 8 {
            let nib = (q >> ((7 - i) * 4)) & 0xf;
            hb[i] = if nib < 10 {
                b'0' + nib as u8
            } else {
                b'a' + (nib - 10) as u8
            };
            i += 1;
        }
        uart.write(0, &hb);
    }
    uart.write(0, b"\r\n");

    // Exercise the FRW/HCC data path (msg pool -> HCC -> worker -> handler).
    let f = ws63_rf_rs::frw_hcc_selftest(); // [sent, received, dispatched, checksum_ok]
    let flabels: [&[u8]; 4] = [
        b"frw sent        = ",
        b"frw received    = ",
        b"frw dispatched  = ",
        b"frw checksum_ok = ",
    ];
    for (label, v) in flabels.iter().zip(f.iter()) {
        uart.write(0, label);
        uart.write(0, u32dec(*v, &mut b));
        uart.write(0, b"\r\n");
    }

    // Exercise the software-timer service (one-shot fire + no-refire + re-arm).
    let tm = ws63_rf_rs::timer_selftest(); // [after_oneshot, after_rearm, ok]
    let tlabels: [&[u8]; 3] = [
        b"timer oneshot   = ",
        b"timer rearm     = ",
        b"timer ok        = ",
    ];
    for (label, v) in tlabels.iter().zip(tm.iter()) {
        uart.write(0, label);
        uart.write(0, u32dec(*v, &mut b));
        uart.write(0, b"\r\n");
    }

    let ok = r == [5, 5, 3, 4] && q == 0xCAFE_F00D && f == [5, 5, 5, 1] && tm == [1, 2, 1];
    uart.write(
        0,
        if ok {
            b"SCHED SELFTEST: PASS\r\n"
        } else {
            b"SCHED SELFTEST: FAIL\r\n"
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
