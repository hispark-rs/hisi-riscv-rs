//! QEMU regression: a stale TIMER0 expiry must not satisfy a new 50 ms delay.
//! TIMER1 supplies an independent modeled 24 MHz reference; this does not
//! qualify physical clock accuracy, asynchronous EOI or board timing.
#![no_std]
#![no_main]
use embedded_hal::delay::DelayNs;
use hisi_hal::{Peripherals, timer::{TimerChannel, TimerDriver, TimerMode}, uart::{Config, Uart}};
use hisi_riscv_rt::entry;
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let uart = Uart::new_uart0(p.UART0, Config::default());
    let driver = TimerDriver::new(p.TIMER);
    driver.configure(TimerChannel::Channel1, TimerMode::OneShot, 24_000_000);
    driver.enable(TimerChannel::Channel1);
    let mut timer = driver.oneshot(TimerChannel::Channel0);
    // Leave a real previous expiry pending before beginning the new wait.
    timer.start(24_000);
    while !timer.expired() {}
    let before = driver.current_value(TimerChannel::Channel1);
    timer.delay_ms(50);
    let after = driver.current_value(TimerChannel::Channel1);
    if before.wrapping_sub(after) >= 1_200_000 { uart.write(b"STALE OK\r\n"); } else { uart.write(b"STALE FAILED\r\n"); }
    loop { core::hint::spin_loop(); }
}
#[panic_handler] fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
