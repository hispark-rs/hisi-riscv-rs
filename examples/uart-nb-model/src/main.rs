//! QEMU-only nonblocking TX contract; UART0 console, UART1 virtual peer.
#![no_std]
#![no_main]

use embedded_hal_nb::serial::Write;
use hisi_hal::{
    Peripherals,
    uart::{Config, Uart},
};
use hisi_riscv_rt::entry;

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let console = Uart::new_uart0(p.UART0, Config::default());
    let mut port = Uart::new_uart1(p.UART1, Config::default());
    console.write(b"UART NB: BEGIN\r\n");
    // Host stalls UART1 before CPU starts; constructor must not undo the fault.
    let mut accepted = 0;
    loop {
        match Write::write(&mut port, accepted as u8) {
            Ok(()) => {
                accepted += 1;
                assert!(accepted <= 16);
            }
            Err(nb::Error::WouldBlock) => break,
            Err(nb::Error::Other(_)) => panic!("unexpected TX error"),
        }
    }
    assert!(accepted == 1 || accepted == 16);
    assert!(matches!(
        Write::flush(&mut port),
        Err(nb::Error::WouldBlock)
    ));
    for _ in 0..4 {
        assert!(matches!(
            Write::write(&mut port, 0xee),
            Err(nb::Error::WouldBlock)
        ));
    }
    console.write(b"UART NB: FULL\r\n");
    // Host releases only after seeing FULL; the driver itself must not spin in write.
    loop {
        match Write::flush(&mut port) {
            Ok(()) => break,
            Err(nb::Error::WouldBlock) => core::hint::spin_loop(),
            Err(nb::Error::Other(_)) => panic!("unexpected flush error"),
        }
    }
    assert!(Write::write(&mut port, 0xa5).is_ok());
    assert!(Write::flush(&mut port).is_ok());
    console.write(b"UART NB: PASS\r\n");
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
