//! Isolate divider access from RX occupancy; internal loopback, no pad changes.
use super::*;

static mut SAMPLES: [u32; 324] = [0; 324];

pub fn check<T: UartInstance>(
    uart: &mut Uart<'_, T>,
    timer: &mut TimerDriver<'_>,
    regs: &ws53_pac::uart0::RegisterBlock,
    port: usize,
) {
    let mut delay = Delay(timer);
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
    for phase in 0..6 {
        if phase == 1 || phase == 2 {
            uart.try_write_byte(0x15 + phase as u8).unwrap();
            drain(uart, regs);
            for _ in 0..100_000 {
                if regs.rx_fifo_cnt().read().bits() == phase as u32 {
                    break;
                }
            }
            assert_eq!(regs.rx_fifo_cnt().read().bits(), phase as u32);
        } else if phase == 3 {
            assert_eq!(uart.read_byte(), Ok(0x16));
            assert_eq!(uart.read_byte(), Ok(0x17));
            assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));
        }
        if phase == 4 {
            for byte in 0..64 {
                uart.try_write_byte(byte).unwrap();
                drain(uart, regs);
                for _ in 0..100_000 {
                    if regs.rx_fifo_cnt().read().bits() == u32::from(byte) + 1 {
                        break;
                    }
                }
                assert_eq!(regs.rx_fifo_cnt().read().bits(), u32::from(byte) + 1);
            }
        } else if phase == 5 {
            for byte in 0..64 {
                assert_eq!(uart.read_byte(), Ok(byte));
            }
            assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));
        }
        delay.delay_us(200); // Settle after TX; not a silicon timing bound.
        for enable in 0..2 {
            let before = regs.uart_ctl().read().bits();
            regs.uart_ctl().modify(|_, w| w.div_en().bit(enable != 0));
            delay.delay_us(10);
            let words = [
                before,
                regs.uart_ctl().read().bits(),
                regs.div_l().read().bits(),
                regs.div_h().read().bits(),
                regs.fifo_status().read().bits(),
                regs.rx_fifo_cnt().read().bits(),
                regs.tx_fifo_cnt().read().bits(),
                regs.line_status().read().bits(),
                regs.div_fra().read().bits(),
            ];
            regs.uart_ctl()
                .modify(|_, w| w.div_en().bit(before & 1 != 0));
            for (i, value) in words.into_iter().enumerate() {
                unsafe {
                    core::ptr::addr_of_mut!(SAMPLES)
                        .cast::<u32>()
                        .add((port * 12 + phase * 2 + enable) * 9 + i)
                        .write_volatile(value);
                }
            }
        }
    }
}

pub fn emit_results(uart: &Uart<'_, Uart2<'_>>) {
    for row in 0..36 {
        super::super::super::emit(uart, b"WS53 UART ACCESS ");
        super::super::super::emit(uart, &[b'0' + row as u8 / 10, b'0' + row as u8 % 10, b' ']);
        for i in 0..9 {
            let value = unsafe {
                core::ptr::addr_of!(SAMPLES)
                    .cast::<u32>()
                    .add(row * 9 + i)
                    .read_volatile()
            };
            let mut hex = [0; 8];
            for (j, ch) in hex.iter_mut().enumerate() {
                *ch = b"0123456789abcdef"[((value >> (28 - 4 * j)) & 15) as usize];
            }
            super::super::super::emit(uart, &hex);
        }
        super::super::super::emit(uart, b"\r\n");
    }
    super::super::super::emit(uart, b"WS53 UART ACCESS OBSERVED ports=3 phases=6\r\n");
}
