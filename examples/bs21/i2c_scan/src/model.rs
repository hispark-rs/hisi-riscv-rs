//! QEMU-only smoke: successful virtual read and virtual address NACK.
//! The model does not qualify bus waveforms, abort causes other than NACK, or recovery.
#![no_std]
#![no_main]

use hisi_hal::{
    Peripherals,
    i2c::{I2c, I2cError, Speed},
    uart::{Config, Uart},
};
use hisi_riscv_rt::entry;

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let uart = Uart::new_uart0(p.UART0, Config::default());
    let mut bus = I2c::new_i2c0(p.I2C0, Speed::Standard);
    let mut data = [0];
    if bus.write_read(0x50, &[0x12], &mut data).is_ok()
        && data == [0xa5]
        && matches!(bus.write(0x51, &[0x34]), Err(I2cError::Nack))
    {
        uart.write(b"BS21 I2C MODEL PASS\r\n");
    } else {
        uart.write(b"BS21 I2C MODEL FAIL\r\n");
    }
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
