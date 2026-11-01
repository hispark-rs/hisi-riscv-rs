//! Explicit QEMU fault-injection scenario, never part of the default image.
use hisi_hal::uart::{Uart, UartInstance};

pub fn check<T: UartInstance>(uart: &mut Uart<'_, T>) {
    // The caller owns this UART and has stopped any previous users. Only this
    // test uses the PAC escape hatch to inject FIFO errors; HAL consumes them.
    let regs = unsafe { uart.register_block() };
    unsafe {
        regs.intr_en().write_with_zero(|w| w);
        regs.fifo_ctl()
            .write_with_zero(|w| w.fifo_en().set_bit().rx_fifo_rst().set_bit());
    }
    regs.far().modify(|_, w| w.far().set_bit());
    let push = |byte: u8, parity: bool, framing: bool| unsafe {
        regs.rx_fifo_write().write_with_zero(|w| {
            w.rx_fifo_write()
                .bits(byte)
                .rx_fifo_pe()
                .bit(parity)
                .rx_fifo_fe()
                .bit(framing)
        });
    };
    assert!(matches!(
        embedded_hal_nb::serial::Read::read(uart),
        Err(nb::Error::WouldBlock)
    ));
    push(0x11, false, false);
    let mut data = [0xa5; 4];
    assert_eq!(embedded_io::Read::read(uart, &mut data), Ok(1));
    assert_eq!(data, [0x11, 0xa5, 0xa5, 0xa5]);

    push(0x22, true, true);
    push(0x33, false, false);
    assert!(uart.tx_flushed());
    assert!(uart.tx_flushed()); // Must not discard the good head using old flags.
    assert_eq!(embedded_io::Read::read(uart, &mut []), Ok(0));
    let Err(nb::Error::Other(error)) = embedded_hal_nb::serial::Read::read(uart) else {
        panic!()
    };
    assert!(error.parity() && error.framing() && error.fifo_error() && !error.overrun());
    assert_eq!(uart.read_byte(), Ok(0x33));
    assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));

    // FIFO-wide error summary must not reject the good prefix. A read-clear
    // error after that prefix must survive an intervening zero-length read.
    push(0x44, false, false);
    push(0x55, true, false);
    push(0x66, false, false);
    assert!(uart.tx_flushed()); // FIFO summary alone must leave the good prefix.
    data.fill(0xa5);
    assert_eq!(embedded_io::Read::read(uart, &mut data), Ok(1));
    assert_eq!(data, [0x44, 0xa5, 0xa5, 0xa5]);
    assert_eq!(embedded_io::Read::read(uart, &mut []), Ok(0));
    assert!(uart.tx_flushed()); // Preserve error already deferred by partial Read.
    let error = embedded_io::Read::read(uart, &mut data).unwrap_err();
    assert!(error.parity() && !error.framing());
    assert_eq!(embedded_io::Read::read(uart, &mut data), Ok(1));
    assert_eq!(data[0], 0x66);
    assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));

    for byte in 0..64 {
        push(byte, false, false);
    }
    push(0xff, false, false);
    assert!(uart.tx_flushed()); // Capture overrun without consuming queued bytes.
    let Err(nb::Error::Other(error)) = uart.read_byte() else {
        panic!()
    };
    assert!(error.overrun() && !error.parity() && !error.framing());
    for byte in 0..64 {
        assert_eq!(uart.read_byte(), Ok(byte), "overrun discarded queued data");
    }
    assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));
    // Repeated TX polls aggregate errors but consume each corrupt head only once.
    push(0x71, true, false);
    push(0x72, false, true);
    push(0x73, false, false);
    assert!(uart.tx_flushed());
    assert!(uart.tx_flushed());
    assert!(uart.tx_flushed());
    let Err(nb::Error::Other(error)) = uart.read_byte() else {
        panic!()
    };
    assert!(error.parity() && error.framing());
    assert_eq!(uart.read_byte(), Ok(0x73));
    assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));
    regs.far().modify(|_, w| w.far().clear_bit());
}
