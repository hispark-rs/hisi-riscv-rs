//! BS2X PWM + WDT example (blocking).
//!
//! Exercises two already-register-verified BS2X drivers (PWM v151, WDT v151 — same
//! IP as WS63) end-to-end on QEMU:
//! - WDT: configure a timeout, enable, feed, then read the counter. The QEMU WDT
//!   model returns the loaded value, so counter_value() is non-zero.
//! - PWM: configure channel 0 (1 kHz, 50%), enable + start. The writes land in the
//!   M_CTL fabric (absorbed by the bs2x machine); the test just checks it runs.

#![no_std]
#![no_main]

use hisi_riscv_hal::Peripherals;
use hisi_riscv_hal::pwm::{Duty, PwmChannel, PwmPeriod};
use hisi_riscv_hal::uart::{Config as UartConfig, Uart};
use hisi_riscv_hal::wdt::{ResetPulseLength, Watchdog, WdtMode, WdtTimeout};
use hisi_riscv_rt::entry;

fn put_hex32(uart: &Uart<'_, impl core::any::Any>, v: u32) {
    let mut buf = [b'0', b'x', 0, 0, 0, 0, 0, 0, 0, 0];
    for i in 0..8 {
        let nib = (v >> ((7 - i) * 4)) & 0xF;
        buf[2 + i] = if nib < 10 { b'0' + nib as u8 } else { b'a' + (nib - 10) as u8 };
    }
    uart.write(0, &buf);
}

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let uart = Uart::new_uart0(p.UART0, UartConfig::default());
    uart.write(0, b"\r\nBS2X PWM + WDT\r\n");

    // WDT — configure a 100 ms timeout, feed, read the counter back.
    let mut wdt = Watchdog::new(p.WDT);
    let _ = wdt.configure(
        WdtTimeout::from_ms(100).unwrap(),
        WdtMode::SingleInterrupt,
        false,
        ResetPulseLength::Cycles2,
    );
    wdt.enable();
    wdt.feed();
    let cnt = wdt.counter_value().unwrap_or(0);
    uart.write(0, b"  wdt counter = ");
    put_hex32(&uart, cnt);
    uart.write(0, b"\r\n");
    let wdt_ok = cnt != 0;

    // PWM — configure + start channel 0; just needs to run without faulting.
    let pwm = p.PWM;
    let mut ch = PwmChannel::new(&pwm, 0);
    ch.configure(PwmPeriod::try_from_hz(1_000).unwrap(), Duty::from_percent(50).unwrap());
    ch.enable();
    ch.start();
    uart.write(0, b"  pwm ch0 started (1 kHz, 50%)\r\n");

    if wdt_ok {
        uart.write(0, b"  PWM+WDT OK\r\n");
    } else {
        uart.write(0, b"  PWM+WDT MISMATCH\r\n");
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
