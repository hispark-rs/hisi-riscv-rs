//! HAL reinitialization and no-side-effect refusal paths, all application UARTs.
use super::*;
use hisi_hal::uart::{ResetError, ResettableUart, SignalPath};

pub fn check<T: ResettableUart>(
    uart: &mut Uart<'_, T>,
    delay: &mut Delay<'_, '_>,
    regs: &ws53_pac::uart0::RegisterBlock,
) {
    let config = LineConfig::new(
        NonZeroU32::new(32_000_000).unwrap(),
        2_000_000,
        DataBits::Seven,
        Parity::Odd,
        StopDuration::Two,
    )
    .unwrap();
    let reset_word = || match T::PORT {
        UartPort::Uart2 => unsafe { &*ws53_pac::GlbCtl::PTR }
            .uart_soft_reset()
            .read()
            .bits(),
        _ => unsafe { &*ws53_pac::MCrg::PTR }
            .uart_soft_reset()
            .read()
            .bits(),
    };
    let before = reset_word();
    // Both vendor clients are quiesced by the RAM runner. A nonzero scratch
    // sentinel detects an unintended neighboring reset, unlike a zero default.
    let peer = unsafe {
        match T::PORT {
            UartPort::Uart0 => &*ws53_pac::Uart1::PTR,
            UartPort::Uart1 => &*ws53_pac::Uart0::PTR,
            UartPort::Uart2 => &*ws53_pac::Uart0::PTR,
        }
    };
    let console = unsafe {
        if T::PORT == UartPort::Uart2 {
            &*ws53_pac::Uart1::PTR
        } else {
            &*ws53_pac::Uart2::PTR
        }
    };
    let saved = [
        (
            peer.uart_gp_reg().read().bits(),
            peer.uart_ctl().read().bits(),
            peer.modem_ctl().read().bits(),
        ),
        (
            console.uart_gp_reg().read().bits(),
            console.uart_ctl().read().bits(),
            console.modem_ctl().read().bits(),
        ),
    ];
    unsafe {
        peer.uart_gp_reg()
            .write_with_zero(|w| w.uart_gp_reg().bits(0xa6));
        console
            .uart_gp_reg()
            .write_with_zero(|w| w.uart_gp_reg().bits(0x3c));
    }
    unsafe {
        regs.uart_gp_reg()
            .write_with_zero(|w| w.uart_gp_reg().bits(0xa5));
    }
    uart.set_receive_interrupts(true);
    assert_eq!(
        unsafe { uart.reset_and_configure(config, SignalPath::InternalLoopback, delay) },
        Err(ResetError::InterruptsEnabled)
    );
    assert_eq!(regs.uart_gp_reg().read().bits(), 0xa5);
    uart.set_receive_interrupts(false);
    uart.try_write_byte(0x13).unwrap();
    assert_eq!(
        unsafe { uart.reset_and_configure(config, SignalPath::InternalLoopback, delay) },
        Err(ResetError::TransmitBusy)
    );
    assert_eq!(regs.uart_gp_reg().read().bits(), 0xa5);
    assert_eq!(reset_word(), before);
    drain(uart, regs);
    let mut received = None;
    for _ in 0..100_000 {
        if let Ok(byte) = uart.read_byte() {
            received = Some(byte);
            break;
        }
    }
    assert_eq!(received, Some(0x13));
    uart.try_write_byte(0x15).unwrap();
    drain(uart, regs);
    for _ in 0..100_000 {
        if regs.rx_fifo_cnt().read().bits() == 1 {
            break;
        }
    }
    assert_eq!(regs.rx_fifo_cnt().read().bits(), 1);
    unsafe {
        uart.reset_and_configure(config, SignalPath::InternalLoopback, delay)
            .unwrap();
    }
    assert_eq!(reset_word(), before);
    assert_eq!(regs.uart_gp_reg().read().bits(), 0);
    assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));
    uart.try_write_byte(0x3a).unwrap();
    drain(uart, regs);
    received = None;
    for _ in 0..100_000 {
        if let Ok(byte) = uart.read_byte() {
            received = Some(byte);
            break;
        }
    }
    assert_eq!(received, Some(0x3a));
    assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));
    for (index, (other, marker)) in [(peer, 0xa6), (console, 0x3c)].into_iter().enumerate() {
        assert_eq!(other.uart_gp_reg().read().bits(), marker);
        assert_eq!(other.uart_ctl().read().bits(), saved[index].1);
        assert_eq!(other.modem_ctl().read().bits(), saved[index].2);
        unsafe {
            other
                .uart_gp_reg()
                .write_with_zero(|w| w.uart_gp_reg().bits(saved[index].0 as u8));
        }
    }
}
