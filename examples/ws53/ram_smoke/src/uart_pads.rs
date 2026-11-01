//! Bounded physical TX-route observation, not RX or cold-start qualification.
//! Both vendor clients are stopped by the RAM runner. Only the three confirmed
//! analyzer input pads and the existing UART2 console TX are driven. Reset on
//! every exit remains mandatory; never resume the overwritten vendor context.
#[cfg(not(feature = "uart-h0-init-pins"))]
use super::*;
use hisi_hal::{
    pinctrl::{Config, PadControl, Pull},
    soc::ws53::pins::{GpioBank, MuxMode, Pad},
};

fn input_only(pad: Pad, restore: Option<bool>) -> bool {
    let line = pad.gpio_line().unwrap();
    let mask = line.mask();
    macro_rules! update {
        ($pac:ident) => {{
            let r = unsafe { &*ws53_pac::$pac::PTR };
            let before = u32::from(r.gpio_sw_oen().read().pins().bits()) & mask != 0;
            let input = restore.unwrap_or(true);
            r.gpio_sw_oen().modify(|old, w| unsafe {
                let value = u32::from(old.pins().bits());
                w.pins()
                    .bits((if input { value | mask } else { value & !mask }) as _)
            });
            assert_eq!(
                u32::from(r.gpio_sw_oen().read().pins().bits()) & mask != 0,
                input
            );
            before
        }};
    }
    match line.bank() {
        GpioBank::Bank0 => update!(Gpio0),
        GpioBank::Bank1 => update!(Gpio1),
        GpioBank::Bank2 => panic!("no secure/debug pad in UART observation"),
    }
}

pub(super) fn prepare(pad: Pad, mode: u8) -> (PadControl<'static>, Config, bool) {
    // Exclusive bounded takeover, with GPIO output disabled before any pad
    // change. The caller has drained TX and isolated non-console UARTs in LB.
    let direction = input_only(pad, None);
    let mut owner = unsafe { PadControl::claim(pad) }.unwrap();
    let saved = owner.configuration().unwrap();
    let config = Config::new(
        pad.select_mode(MuxMode::from_code(mode).unwrap()).unwrap(),
        pad.drive_setting(0).unwrap(),
        Pull::Up,
        true,
        saved.schmitt_enabled(),
    )
    .unwrap();
    owner.configure(config).unwrap();
    assert_eq!(owner.configuration().unwrap(), config);
    (owner, saved, direction)
}
pub(super) fn restore((mut owner, config, direction): (PadControl<'_>, Config, bool)) {
    owner.configure(config).unwrap();
    assert_eq!(owner.configuration().unwrap(), config);
    input_only(owner.pad(), Some(direction));
}

#[cfg(not(feature = "uart-h0-init-pins"))]
pub fn check<T: UartInstance>(
    uart: &mut Uart<'_, T>,
    timer: &mut TimerDriver<'_>,
    regs: &ws53_pac::uart0::RegisterBlock,
    route: u8,
) {
    let (port, tx, rx, mode) = match route {
        0 => (UartPort::Uart0, Pad::Mgpio12, Pad::Agpio4, 2),
        1 => (UartPort::Uart1, Pad::Mgpio17, Pad::Mgpio18, 2),
        2 => (UartPort::Uart1, Pad::Mgpio8, Pad::Mgpio9, 1),
        _ => panic!("unknown route"),
    };
    assert_eq!(T::PORT, port);
    drain(uart, regs);
    assert!(regs.modem_ctl().read().lb_mode().bit());
    let tx = prepare(tx, mode);
    let rx = prepare(rx, mode);
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    let mut delay = Delay(timer);
    unsafe {
        regs.modem_ctl().write_with_zero(|w| w);
    }
    delay.delay_ms(2);
    // Distinct route ID followed by every possible byte; independently decoded
    // from the DSLogic sample clock at 115200 8N1, not from register readback.
    for byte in [0x55, 0xaa, b'W', b'S', b'5', b'3', route, 0xc3] {
        uart.try_write_byte(byte).unwrap();
        drain(uart, regs);
    }
    for byte in 0..=255u8 {
        uart.try_write_byte(byte).unwrap();
        drain(uart, regs);
    }
    delay.delay_ms(2);
    unsafe {
        regs.modem_ctl().write_with_zero(|w| w.lb_mode().set_bit());
    }
    restore(rx);
    restore(tx);
    delay.0.disable(TimerChannel::Channel1);
}

#[cfg(feature = "uart-pad-probe")]
pub fn console(uart: &Uart<'_, Uart2<'_>>) {
    let regs = unsafe { &*ws53_pac::Uart2::PTR };
    drain(uart, regs);
    assert!(!regs.modem_ctl().read().lb_mode().bit());
    // The console already uses this mux. Verify it before reapplying pad fields;
    // do not switch a live console from an unexpected route.
    for pad in [Pad::Agpio1, Pad::Agpio2] {
        let owner = unsafe { PadControl::claim(pad) }.unwrap();
        assert_eq!(owner.configuration().unwrap().mode().mode().code(), 1);
    }
    let tx = prepare(Pad::Agpio1, 1);
    let rx = prepare(Pad::Agpio2, 1);
    super::super::emit(uart, b"WS53 UART PAD console configured=1\r\n");
    drain(uart, regs);
    restore(rx);
    restore(tx);
}
