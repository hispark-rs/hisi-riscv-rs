//! Hardware observation of H1/H0 soft reset; never reset the UART2 console.
use super::*;
#[cfg(feature = "uart-access-probe")]
#[path = "uart_access.rs"]
pub(super) mod access;

#[cfg(feature = "uart-hal-reset-probe")]
#[path = "uart_hal_reset.rs"]
mod hal_reset;

static mut SAMPLES: [u32; 56] = [0; 56];

fn capture(regs: &ws53_pac::uart0::RegisterBlock, row: usize, delay: &mut Delay<'_, '_>) {
    let ctl = regs.uart_ctl().read().bits();
    regs.uart_ctl().modify(|_, w| w.div_en().set_bit());
    delay.delay_us(10); // Separate divider access settling from reset effects.
    let low = regs.div_l().read().bits();
    let high = regs.div_h().read().bits();
    regs.uart_ctl().modify(|_, w| w.div_en().clear_bit());
    let words = [
        ctl,
        low,
        high,
        regs.div_fra().read().bits(),
        regs.baud_ctl().read().bits(),
        regs.modem_ctl().read().bits(),
        regs.intr_en().read().bits(),
        regs.fifo_status().read().bits(),
        regs.tx_fifo_cnt().read().bits(),
        regs.rx_fifo_cnt().read().bits(),
        regs.uart_gp_reg().read().bits(),
        regs.uart_parameter().read().bits(),
        regs.line_status().read().bits(),
        regs.intr_id().read().bits(),
    ];
    for (i, value) in words.into_iter().enumerate() {
        unsafe {
            core::ptr::addr_of_mut!(SAMPLES)
                .cast::<u32>()
                .add(row * 14 + i)
                .write_volatile(value)
        }
    }
}

pub fn check<T: hisi_hal::uart::ResettableUart>(
    uart: &mut Uart<'_, T>,
    timer: &mut TimerDriver<'_>,
    regs: &ws53_pac::uart0::RegisterBlock,
) {
    let port = match T::PORT {
        UartPort::Uart0 => 0,
        UartPort::Uart1 => 1,
        _ => panic!("console reset forbidden"),
    };
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    #[cfg(feature = "uart-access-probe")]
    access::check(uart, timer, regs, port);
    let mut delay = Delay(timer);
    unsafe {
        uart.configure_line(
            LineConfig::new(
                NonZeroU32::new(32_000_000).unwrap(),
                2_500_000,
                DataBits::Seven,
                Parity::Odd,
                StopDuration::Two,
            )
            .unwrap(),
            &mut delay,
        );
    }
    uart.try_write_byte(0x15).unwrap();
    drain(uart, regs);
    // Keep one real received character queued across the reset command.
    let mut queued = false;
    for _ in 0..100_000 {
        if regs.rx_fifo_cnt().read().bits() == 1 {
            queued = true;
            break;
        }
    }
    assert!(queued);
    unsafe {
        regs.uart_gp_reg()
            .write_with_zero(|w| w.uart_gp_reg().bits(0x5a));
    }
    capture(regs, port * 2, &mut delay);
    let crg = unsafe { &*ws53_pac::MCrg::PTR };
    let before = crg.uart_soft_reset().read().bits();
    assert_eq!(before & 3, 3);
    match T::PORT {
        UartPort::Uart0 => {
            crg.uart_soft_reset()
                .modify(|_, w| w.uart_h1_reset_n().clear_bit());
            crg.uart_soft_reset()
                .modify(|_, w| w.uart_h1_reset_n().set_bit());
        }
        UartPort::Uart1 => {
            crg.uart_soft_reset()
                .modify(|_, w| w.uart_h0_reset_n().clear_bit());
            crg.uart_soft_reset()
                .modify(|_, w| w.uart_h0_reset_n().set_bit());
        }
        _ => unreachable!(),
    }
    assert_eq!(crg.uart_soft_reset().read().bits(), before);
    delay.delay_us(10); // Observation margin only; no minimum reset timing claim.
    capture(regs, port * 2 + 1, &mut delay);
    // Reset values are observations, not asserted assumptions. Reprogram the
    // owned instance and prove it can receive another internal-loopback byte.
    prepare_loopback(regs);
    unsafe {
        uart.configure_line(
            LineConfig::new(
                NonZeroU32::new(32_000_000).unwrap(),
                115200,
                DataBits::Eight,
                Parity::None,
                StopDuration::One,
            )
            .unwrap(),
            &mut delay,
        );
    }
    uart.try_write_byte(0x0a).unwrap();
    drain(uart, regs);
    let mut received = None;
    for _ in 0..100_000 {
        match uart.read_byte() {
            Ok(byte) => {
                received = Some(byte);
                break;
            }
            Err(nb::Error::WouldBlock) => (),
            Err(nb::Error::Other(_)) => panic!("post-reset receive error"),
        }
    }
    assert_eq!(received, Some(0x0a));
    assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));
    #[cfg(feature = "uart-hal-reset-probe")]
    hal_reset::check(uart, &mut delay, regs);
    delay.0.disable(TimerChannel::Channel1);
}

pub fn emit_results(uart: &Uart<'_, Uart2<'_>>) {
    for row in 0..4 {
        super::super::emit(uart, b"WS53 UART RESET ");
        super::super::emit(uart, &[b'0' + row as u8, b' ']);
        for i in 0..14 {
            let word = unsafe {
                core::ptr::addr_of!(SAMPLES)
                    .cast::<u32>()
                    .add(row * 14 + i)
                    .read_volatile()
            };
            let mut hex = [0u8; 8];
            for (j, ch) in hex.iter_mut().enumerate() {
                *ch = b"0123456789abcdef"[((word >> (28 - 4 * j)) & 15) as usize];
            }
            super::super::emit(uart, &hex);
        }
        super::super::emit(uart, b"\r\n");
    }
    super::super::emit(uart, b"WS53 UART RESET OBSERVED ports=2 recovery=2\r\n");
    #[cfg(feature = "uart-access-probe")]
    access::emit_results(uart);
    #[cfg(feature = "uart-hal-reset-probe")]
    super::super::emit(
        uart,
        b"WS53 PASS uart_hal_reset ports=2 rejected=4 recovery=2 peers=4\r\n",
    );
}
