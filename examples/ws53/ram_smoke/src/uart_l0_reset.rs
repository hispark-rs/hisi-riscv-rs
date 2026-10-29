//! L0 raw-reset observation; recover the UART2 console before printing results.
use super::*;
#[cfg(feature = "uart-l0-hal-reset-probe")]
#[path = "uart_hal_reset.rs"]
mod hal_reset;

static mut SAMPLES: [u32; 28] = [0; 28];

fn capture(r: &ws53_pac::uart0::RegisterBlock, row: usize, delay: &mut Delay<'_, '_>) {
    let ctl = r.uart_ctl().read().bits();
    r.uart_ctl().modify(|_, w| w.div_en().set_bit());
    delay.delay_us(10);
    let low = r.div_l().read().bits();
    let high = r.div_h().read().bits();
    r.uart_ctl().modify(|_, w| w.div_en().clear_bit());
    let values = [
        ctl,
        low,
        high,
        r.div_fra().read().bits(),
        r.baud_ctl().read().bits(),
        r.modem_ctl().read().bits(),
        r.intr_en().read().bits(),
        r.fifo_status().read().bits(),
        r.tx_fifo_cnt().read().bits(),
        r.rx_fifo_cnt().read().bits(),
        r.uart_gp_reg().read().bits(),
        r.uart_parameter().read().bits(),
        r.line_status().read().bits(),
        r.intr_id().read().bits(),
    ];
    for (i, word) in values.into_iter().enumerate() {
        unsafe {
            core::ptr::addr_of_mut!(SAMPLES)
                .cast::<u32>()
                .add(row * 14 + i)
                .write_volatile(word);
        }
    }
}

pub fn check(uart: &mut Uart<'_, Uart2<'_>>, timer: &mut TimerDriver<'_>) {
    state(0, 100);
    let r = unsafe { &*ws53_pac::Uart2::PTR };
    let peers = [unsafe { &*ws53_pac::Uart0::PTR }, unsafe {
        &*ws53_pac::Uart1::PTR
    }];
    let saved = peers.map(|p| {
        [
            p.uart_gp_reg().read().bits(),
            p.uart_ctl().read().bits(),
            p.modem_ctl().read().bits(),
        ]
    });
    for (i, peer) in peers.iter().enumerate() {
        unsafe {
            peer.uart_gp_reg()
                .write_with_zero(|w| w.uart_gp_reg().bits(0xa5 + i as u8));
        }
    }
    drain(uart, r);
    prepare_loopback(r);
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
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
        r.uart_gp_reg()
            .write_with_zero(|w| w.uart_gp_reg().bits(0x5a));
    }
    uart.try_write_byte(0x15).unwrap();
    drain(uart, r);
    for _ in 0..100_000 {
        if r.rx_fifo_cnt().read().bits() == 1 {
            break;
        }
    }
    assert_eq!(r.rx_fifo_cnt().read().bits(), 1);
    state(0, 101);
    capture(r, 0, &mut delay);
    let glb = unsafe { &*ws53_pac::GlbCtl::PTR };
    let before = glb.uart_soft_reset().read().bits();
    assert!(glb.uart_soft_reset().read().uart_l0_reset_n().bit());
    glb.uart_soft_reset()
        .modify(|_, w| w.uart_l0_reset_n().clear_bit());
    glb.uart_soft_reset()
        .modify(|_, w| w.uart_l0_reset_n().set_bit());
    assert_eq!(glb.uart_soft_reset().read().bits(), before);
    delay.delay_us(10);
    state(0, 102);
    capture(r, 1, &mut delay);
    prepare_loopback(r);
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
    drain(uart, r);
    let mut received = None;
    for _ in 0..100_000 {
        if let Ok(byte) = uart.read_byte() {
            received = Some(byte);
            break;
        }
    }
    assert_eq!(received, Some(0x0a));
    assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));
    #[cfg(feature = "uart-l0-hal-reset-probe")]
    {
        hal_reset::check(uart, &mut delay, r);
        unsafe {
            uart.reset_and_configure(
                LineConfig::new(
                    NonZeroU32::new(32_000_000).unwrap(),
                    115200,
                    DataBits::Eight,
                    Parity::None,
                    StopDuration::One,
                )
                .unwrap(),
                hisi_hal::uart::SignalPath::Pins,
                &mut delay,
            )
            .unwrap();
        }
    }
    state(0, 103);
    for (i, peer) in peers.iter().enumerate() {
        assert_eq!(peer.uart_gp_reg().read().bits(), 0xa5 + i as u32);
        assert_eq!(peer.uart_ctl().read().bits(), saved[i][1]);
        assert_eq!(peer.modem_ctl().read().bits(), saved[i][2]);
        unsafe {
            peer.uart_gp_reg()
                .write_with_zero(|w| w.uart_gp_reg().bits(saved[i][0] as u8));
        }
    }
    unsafe {
        r.modem_ctl().write_with_zero(|w| w);
    }
    delay.0.disable(TimerChannel::Channel1);
    state(0, 11);
}

pub fn emit_results(uart: &Uart<'_, Uart2<'_>>) {
    for row in 0..2 {
        super::super::emit(uart, b"WS53 UART L0 RESET ");
        super::super::emit(uart, &[b'0' + row as u8, b' ']);
        for i in 0..14 {
            let value = unsafe {
                core::ptr::addr_of!(SAMPLES)
                    .cast::<u32>()
                    .add(row * 14 + i)
                    .read_volatile()
            };
            let mut hex = [0; 8];
            for (j, ch) in hex.iter_mut().enumerate() {
                *ch = b"0123456789abcdef"[((value >> (28 - j * 4)) & 15) as usize];
            }
            super::super::emit(uart, &hex);
        }
        super::super::emit(uart, b"\r\n");
    }
    super::super::emit(
        uart,
        b"WS53 UART L0 RESET OBSERVED recovery=1 peers=2 console=1\r\n",
    );
    #[cfg(feature = "uart-l0-hal-reset-probe")]
    super::super::emit(
        uart,
        b"WS53 PASS uart_l0_hal_reset rejected=2 recovery=1 peers=2 pins=1\r\n",
    );
}
