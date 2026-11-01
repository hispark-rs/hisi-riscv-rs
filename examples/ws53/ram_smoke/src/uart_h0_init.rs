//! Real public H0 initialization, plus guard refusals retaining Unclaimed.
//! UART MMIO is used only under the inherited clock before this diagnostic
//! explicitly gates it, and after the constructor establishes it again.
use super::*;
use hisi_hal::{
    interrupt,
    peripherals::{Uart1, Unclaimed},
    uart::{H0InitError, SignalPath},
};

#[unsafe(no_mangle)]
pub static mut WS53_H0_INIT: [u32; 32] = [0; 32];
#[unsafe(no_mangle)]
pub static WS53_H0_INIT_CLOCK_ON: u32 = cfg!(feature = "uart-h0-init-already-on") as u32;
#[unsafe(no_mangle)]
pub static WS53_H0_INIT_PINS: u32 = cfg!(feature = "uart-h0-init-pins") as u32;
fn put(i: usize, v: u32) {
    unsafe {
        core::ptr::addr_of_mut!(WS53_H0_INIT)
            .cast::<u32>()
            .add(i)
            .write_volatile(v)
    }
}

pub fn initialize(
    mut token: Unclaimed<Uart1<'static>>,
    timer: &mut TimerDriver<'_>,
) -> Uart<'static, Uart1<'static>> {
    put(0, 0x53c20001);
    put(3, unsafe {
        core::ptr::addr_of!(WS53_H0_INIT_CLOCK_ON).read_volatile()
    });
    put(21, unsafe {
        core::ptr::addr_of!(WS53_H0_INIT_PINS).read_volatile()
    });
    let path = if cfg!(feature = "uart-h0-init-pins") {
        SignalPath::Pins
    } else {
        SignalPath::InternalLoopback
    };
    let crg = unsafe { &*ws53_pac::MCrg::PTR };
    let r = unsafe { &*ws53_pac::Uart1::PTR };
    let irq = UartPort::Uart1.interrupt();
    assert!(!interrupt::is_enabled(irq) && !interrupt::is_pending(irq));
    assert_eq!(
        unsafe { &*ws53_pac::Mdma::PTR }
            .enabled_channels()
            .read()
            .bits(),
        0
    );
    assert!(!riscv::register::mstatus::read().mie());
    let old = crg.m_clken1().read().bits();
    assert_ne!(old & 16, 0);
    put(4, u32::from(old));
    let reset = crg.uart_soft_reset().read().bits();
    assert_ne!(reset & 1, 0);
    put(9, u32::from(reset));
    // Explicit HIL preconditioning while inherited bus/gate is known active.
    // No old software/DMA clients remain; queued RX discard is authorized.
    assert!(r.line_status().read().tx_empty_s().bit());
    prepare_loopback(r);
    #[cfg(feature = "uart-h0-init-pins")]
    let (tx, rx) = {
        use hisi_hal::soc::ws53::pins::Pad;
        (
            super::pads::prepare(Pad::Mgpio8, 1),
            super::pads::prepare(Pad::Mgpio9, 1),
        )
    };
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    if !cfg!(feature = "uart-h0-init-already-on") {
        crg.m_clken1().modify(|_, w| w.uart_h0_enable().clear_bit());
    }
    let prepared = crg.m_clken1().read().bits();
    put(5, u32::from(prepared));
    let config = LineConfig::new(
        NonZeroU32::new(32_000_000).unwrap(),
        115200,
        DataBits::Eight,
        Parity::None,
        StopDuration::One,
    )
    .unwrap();
    unsafe { interrupt::enable(irq) };
    let failure = match unsafe { Uart::initialize_h0(token, config, path, &mut Delay(timer)) } {
        Err(error) => error,
        Ok(_) => panic!("H0 accepted enabled IRQ"),
    };
    assert_eq!(failure.error(), H0InitError::InterruptsEnabled);
    put(17, 1);
    (token, _) = failure.into_parts();
    unsafe { interrupt::disable(irq) };
    put(6, u32::from(crg.m_clken1().read().bits()));
    assert_eq!(crg.m_clken1().read().bits(), prepared);
    crg.uart_soft_reset()
        .modify(|_, w| w.uart_h0_reset_n().clear_bit());
    let failure = match unsafe { Uart::initialize_h0(token, config, path, &mut Delay(timer)) } {
        Err(error) => error,
        Ok(_) => panic!("H0 accepted held reset"),
    };
    assert_eq!(failure.error(), H0InitError::ResetAsserted);
    put(18, 3);
    (token, _) = failure.into_parts();
    put(7, u32::from(crg.m_clken1().read().bits()));
    assert_eq!(crg.m_clken1().read().bits(), prepared);
    crg.uart_soft_reset()
        .modify(|_, w| w.uart_h0_reset_n().set_bit());
    Delay(timer).delay_us(10);
    assert_eq!(crg.uart_soft_reset().read().bits(), reset);
    put(1, 1);
    let uart = unsafe { Uart::initialize_h0(token, config, path, &mut Delay(timer)) }.unwrap();
    #[cfg(not(feature = "uart-h0-init-pins"))]
    let mut uart = uart;
    for (i, value) in [
        u32::from(crg.m_clken1().read().bits()),
        u32::from(crg.uart_soft_reset().read().bits()),
        u32::from(interrupt::is_enabled(irq) || interrupt::is_pending(irq)),
        r.intr_en().read().bits(),
        r.uart_ctl().read().bits(),
        r.modem_ctl().read().bits(),
        u32::from(r.intr_id().read().fifo_en_s().bit()),
        u32::from(uart.tx_flushed()),
    ]
    .into_iter()
    .enumerate()
    {
        put(
            if i == 0 {
                8
            } else if i == 1 {
                10
            } else {
                9 + i
            },
            value,
        )
    }
    assert_eq!(crg.m_clken1().read().bits(), old);
    // Exercise the returned configuration before any line/FIFO reconfiguration.
    put(24, r.modem_ctl().read().bits());
    #[cfg(feature = "uart-h0-init-pins")]
    {
        Delay(timer).delay_ms(2);
        for byte in [0x55, 0xaa, b'W', b'S', b'5', b'3', 2, 0xc3]
            .into_iter()
            .chain(0..=255u8)
        {
            uart.try_write_byte(byte).unwrap();
            drain(&uart, r);
        }
        Delay(timer).delay_ms(2);
        put(22, 264);
        put(23, u32::from(uart.tx_flushed()));
        // Isolate TX before restoring pads, then run the older line regression.
        unsafe {
            r.modem_ctl().write_with_zero(|w| w.lb_mode().set_bit());
        }
        super::pads::restore(rx);
        super::pads::restore(tx);
        put(25, 1);
    }
    #[cfg(not(feature = "uart-h0-init-pins"))]
    {
        for byte in 0..=255u8 {
            uart.try_write_byte(byte).unwrap();
            drain(&uart, r);
            let mut received = None;
            for _ in 0..100_000 {
                match uart.read_byte() {
                    Ok(value) => {
                        received = Some(value);
                        break;
                    }
                    Err(nb::Error::WouldBlock) => {}
                    Err(nb::Error::Other(_)) => panic!("H0 immediate receive error"),
                }
            }
            assert_eq!(received, Some(byte));
        }
        assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));
        put(22, 256);
        put(23, u32::from(uart.tx_flushed()));
    }
    timer.disable(TimerChannel::Channel1);
    timer.clear_interrupt(TimerChannel::Channel1);
    put(1, 2);
    uart
}

pub fn finish() {
    let crg = unsafe { &*ws53_pac::MCrg::PTR };
    put(20, u32::from(crg.m_clken1().read().bits()));
    put(19, 1);
    put(25, 1);
    put(1, 3);
}
