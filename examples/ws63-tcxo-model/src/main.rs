//! QEMU-only smoke fixture; no claim about physical board configuration.
#![no_std]
#![no_main]
use hisi_hal::{Peripherals, tcxo::TcxoDriver};
use hisi_riscv_rt::entry;
fn puts(s: &[u8]) {
    for b in s {
        unsafe { core::ptr::write_volatile(0x4401_0004 as *mut u32, *b as u32) }
    }
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let mut clock = TcxoDriver::new(p.TCXO);
    clock.enable();
    let a = clock.read_counter().unwrap();
    let b = clock.read_counter32().unwrap();
    let us = embassy_time_driver::now();
    let c = clock.read_counter().unwrap();
    if (a as u32) <= b && b <= (c as u32) && a / 24 <= us && us <= c / 24 {
        puts(b"WS63 TCXO EMBASSY PASS\n");
    } else {
        puts(b"WS63 TCXO EMBASSY FAIL\n");
    }
    loop {
        core::hint::spin_loop();
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    puts(b"WS63 PANIC\n");
    loop {
        core::hint::spin_loop();
    }
}
