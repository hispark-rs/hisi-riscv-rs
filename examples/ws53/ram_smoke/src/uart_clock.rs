//! Isolated M_CLKEN1.bit4 observation. Never touch UART MMIO while gated.
//! Source clocks/SDMA quiescence remain inherited and unqualified. The runner
//! must stop vendor CPUs/IRQs and reset the board on every exit.
use super::*;
use hisi_hal::{
    gpio::GpioDriver,
    peripherals::{Gpio0, Uart0, Uart1},
    pinctrl::{Config, PadControl, Pull},
    soc::ws53::pins::{MuxMode, Pad},
};

#[unsafe(no_mangle)]
pub static mut WS53_UART_CLOCK_STATE: [u32; 56] = [0; 56];
fn put(i: usize, v: u32) {
    unsafe {
        core::ptr::addr_of_mut!(WS53_UART_CLOCK_STATE)
            .cast::<u32>()
            .add(i)
            .write_volatile(v)
    }
}
fn pad(p: Pad, mode: u8) -> (PadControl<'static>, Config) {
    let bit = 1u32 << p.gpio_line().unwrap().bit();
    assert_ne!(
        unsafe { &*ws53_pac::Gpio0::PTR }
            .gpio_sw_oen()
            .read()
            .bits()
            & bit,
        0
    );
    let mut owner = unsafe { PadControl::claim(p) }.unwrap();
    let original = owner.configuration().unwrap();
    owner
        .configure(
            Config::new(
                p.select_mode(MuxMode::from_code(mode).unwrap()).unwrap(),
                p.drive_setting(0).unwrap(),
                Pull::Up,
                true,
                original.schmitt_enabled(),
            )
            .unwrap(),
        )
        .unwrap();
    (owner, original)
}
fn restore((mut p, c): (PadControl<'_>, Config)) {
    p.configure(c).unwrap();
    assert_eq!(p.configuration().unwrap(), c)
}

pub fn check(
    u0: &mut Uart<'_, Uart0<'_>>,
    u1: &mut Uart<'_, Uart1<'_>>,
    timer: &mut TimerDriver<'_>,
    gpio: Gpio0<'_>,
) {
    let gate = unsafe { &*ws53_pac::MCrg::PTR }.m_clken1();
    let original = gate.read().bits();
    assert_ne!(original & 16, 0);
    put(0, 0x53c10001);
    put(3, u32::from(original));
    put(6, 2);
    let marker = pad(Pad::Mgpio17, 0);
    let marker_config = marker.1;
    let pin = marker
        .0
        .into_gpio()
        .unwrap_or_else(|_| panic!("clock marker GPIO"));
    let mut gpio = GpioDriver::new_gpio0(gpio);
    {
        let mut marker = gpio
            .init_output(pin, false)
            .unwrap_or_else(|_| panic!("marker output"));
        timer.configure(
            TimerChannel::Channel1,
            TimerMode::Periodic,
            TimerTicks::from_count(u32::MAX).unwrap(),
        );
        timer.enable(TimerChannel::Channel1);
        macro_rules! port {
            ($uart:expr,$pac:ident,$pad:expr,$mode:expr,$id:expr) => {{
                let r = unsafe { &*ws53_pac::$pac::PTR };
                let u = &mut *$uart;
                drain(u, r);
                prepare_loopback(r);
                unsafe {
                    u.configure_line(
                        LineConfig::new(
                            NonZeroU32::new(32_000_000).unwrap(),
                            115200,
                            DataBits::Eight,
                            Parity::None,
                            StopDuration::One,
                        )
                        .unwrap(),
                        &mut Delay(timer),
                    )
                };
                let tx = pad($pad, $mode);
                unsafe { r.modem_ctl().write_with_zero(|w| w) };
                Delay(timer).delay_ms(2);
                for phase in 0usize..3 {
                    drain(u, r);
                    assert_eq!(r.intr_en().read().bits(), 0);
                    for _ in 0..16 {
                        u.try_write_byte(0x55).unwrap()
                    }
                    Delay(timer).delay_us(200);
                    let before = timer.current_value(TimerChannel::Channel1).unwrap();
                    marker.set_high();
                    riscv::asm::fence();
                    if phase == 1 {
                        gate.modify(|_, w| w.uart_h0_enable().clear_bit());
                    }
                    let off = gate.read().bits();
                    // TIMER1/M_CRG/GPIO only until original gate is restored.
                    let start = timer.current_value(TimerChannel::Channel1).unwrap();
                    let mut elapsed = false;
                    for _ in 0..1_000_000 {
                        if start.wrapping_sub(timer.current_value(TimerChannel::Channel1).unwrap())
                            >= 32_000
                        {
                            elapsed = true;
                            break;
                        }
                    }
                    if phase == 1 {
                        gate.modify(|_, w| w.uart_h0_enable().set_bit());
                        riscv::asm::fence();
                    }
                    let on = gate.read().bits();
                    marker.set_low();
                    riscv::asm::fence();
                    let after = timer.current_value(TimerChannel::Channel1).unwrap();
                    assert!(elapsed);
                    assert_eq!(on, original);
                    assert_eq!(off, if phase == 1 { original & !16 } else { original });
                    drain(u, r);
                    let row = 8 + ($id * 3 + phase) * 8;
                    for (i, v) in [
                        $id as u32,
                        phase as u32,
                        before,
                        after,
                        u32::from(off),
                        u32::from(on),
                        1,
                        16,
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        put(row + i, v)
                    }
                    put(1, ($id * 3 + phase + 1) as u32);
                    Delay(timer).delay_ms(2);
                }
                unsafe { r.modem_ctl().write_with_zero(|w| w.lb_mode().set_bit()) };
                restore(tx);
            }};
        }
        port!(u0, Uart0, Pad::Mgpio12, 2, 0usize);
        port!(u1, Uart1, Pad::Mgpio8, 1, 1usize);
        timer.disable(TimerChannel::Channel1);
        timer.clear_interrupt(TimerChannel::Channel1);
    }
    let mut marker = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    marker.configure(marker_config).unwrap();
    assert_eq!(marker.configuration().unwrap(), marker_config);
    put(4, u32::from(gate.read().bits()));
    put(5, 1);
    put(7, 4);
}
