//! Bounded PWM0 finite-burst observation, MIO21/SDK47/D11 only.
//! Same whole-block handoff and alias prechecks as pwm0-smoke.
//! Polling, counter and IRQ72 scenarios are selected explicitly by features.
#[cfg(feature = "pwm0-abnormal-probe")]
#[path = "pwm_fault.rs"]
mod fault;
#[cfg(feature = "pwm0-finite-irq-probe")]
#[path = "pwm_finite_irq.rs"]
mod irq;
#[cfg(all(
    feature = "pwm0-abnormal-probe",
    any(feature = "pwm0-finite-irq-probe", feature = "pwm0-finite-delay-probe")
))]
compile_error!("choose one PWM fault, IRQ or direct-PAC diagnostic fixture");
#[cfg(all(feature = "pwm0-finite-irq-probe", feature = "pwm0-finite-delay-probe"))]
compile_error!("choose the HAL IRQ fixture or direct-PAC diagnostic, not both");
use core::cell::RefCell;
use embedded_hal::delay::DelayNs;
use hisi_hal::peripherals::{Pwm, Uart2};
use hisi_hal::pinctrl::{Config, PadControl, Pull};
#[cfg(not(feature = "pwm0-finite-irq-probe"))]
use hisi_hal::pwm;
#[cfg(not(feature = "pwm0-finite-delay-probe"))]
use hisi_hal::pwm::Error;
use hisi_hal::pwm::{
    Channel, Group, GroupConfig, OutputRoute, PulseCount, PulseMode, PwmDriver, Waveform,
};
use hisi_hal::soc::ws53::pins::{MuxMode, Pad};
use hisi_hal::tcxo::TcxoDriver;
use hisi_hal::uart::Uart;

#[derive(Clone, Copy)]
struct TcxoDelay<'a, 'd>(&'a RefCell<TcxoDriver<'d>>);
impl DelayNs for TcxoDelay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000);
        let first = self.0.borrow_mut().read_counter().unwrap().ticks();
        for _ in 0..1_000_000 {
            let now = self.0.borrow_mut().read_counter().unwrap().ticks();
            if now.checked_sub(first).unwrap() >= ticks {
                return;
            }
        }
        super::fail();
    }
}
fn stage(uart: &mut Uart<'_, Uart2<'_>>, number: u8) {
    super::emit(uart, b"WS53 PWM FINITE STAGE ");
    super::emit(uart, &[b'0' + number]);
    super::emit(uart, b"\r\n");
    for _ in 0..100_000_000 {
        match uart.read_byte() {
            Ok(b'g') => return,
            Err(nb::Error::WouldBlock) => (),
            _ => super::fail(),
        }
    }
    super::fail();
}
fn hex(uart: &mut Uart<'_, Uart2<'_>>, value: u16) {
    for shift in [12, 8, 4, 0] {
        super::emit(
            uart,
            &[b"0123456789abcdef"[((value >> shift) & 15) as usize]],
        );
    }
}
pub fn check(token: Pwm<'_>, mut tcxo: TcxoDriver<'_>, uart: &mut Uart<'_, Uart2<'_>>) {
    tcxo.enable().unwrap();
    let tcxo = RefCell::new(tcxo);
    let delay = TcxoDelay(&tcxo);
    let mut pwm = PwmDriver::new(token, delay);
    #[cfg(feature = "pwm0-abnormal-probe")]
    let fault_ok = fault::check(&mut pwm, delay, uart);
    let pad = Pad::Sgpio2;
    let mut owner = unsafe { PadControl::claim(pad) }.unwrap();
    let original = owner.configuration().unwrap();
    let cfg = Config::new(
        pad.select_mode(MuxMode::from_code(1).unwrap()).unwrap(),
        pad.drive_setting(0).unwrap(),
        Pull::None,
        true,
        true,
    )
    .unwrap();
    owner.configure(cfg).unwrap();
    if owner.configuration().unwrap() != cfg {
        super::fail();
    }
    if pwm
        .attach(unsafe { OutputRoute::from_configured(Channel::C0, owner) })
        .is_err()
    {
        super::fail();
    }
    let mut good = true;
    #[cfg(feature = "pwm0-abnormal-probe")]
    {
        good &= fault_ok;
    }
    for (phase, pulses) in [8, 16].into_iter().enumerate() {
        let wave = Waveform::new(
            100,
            100,
            0,
            PulseMode::Finite(PulseCount::new(pulses).unwrap()),
        )
        .unwrap();
        pwm.configure_group(Group::G0, GroupConfig::new(&[(Channel::C0, wave)]).unwrap())
            .unwrap();
        stage(uart, phase as u8 * 2);
        #[cfg(feature = "pwm0-finite-irq-probe")]
        irq::arm(&mut pwm, phase);
        #[cfg(not(feature = "pwm0-finite-delay-probe"))]
        let outcome = match pwm.start(Group::G0) {
            Ok(()) => 0,
            Err(Error::FiniteDeadlineMissed) => 1,
            Err(_) => 2,
        };
        #[cfg(feature = "pwm0-finite-delay-probe")]
        let outcome = {
            // Diagnostic isolation: both IRQs disabled and entire block owned.
            // Same source sequence, except allow10us after the initial START
            // before overwriting EN. This is not a new production HAL contract.
            let r = unsafe { &*ws53_pac::Pwm::PTR };
            unsafe {
                pwm::acknowledge_interrupt();
            }
            let before = r.pwm_periodload_flag0().read().bits();
            r.pwm_en0().modify(|_, w| w.pwm_en_0().set_bit());
            r.pwm_startclrcnt_en0()
                .modify(|_, w| w.pwm_startclrcnt_en_0().set_bit());
            #[cfg(feature = "pwm0-finite-counter-probe")]
            let origin = tcxo.borrow_mut().read_counter().unwrap().ticks();
            unsafe {
                r.pwm_start0()
                    .write_with_zero(|w| w.pwm_start_0().set_bit());
            }
            let mut wait = delay;
            wait.delay_us(10);
            let loaded = r.pwm_periodload_flag0().read().bits();
            let count = r.pwm_periodcnt0().read().bits();
            unsafe {
                pwm::acknowledge_interrupt();
            }
            let cleared = r.pwm_periodload_flag0().read().bits();
            r.pwm_en0().modify(|_, w| w.pwm_en_0().clear_bit());
            wait.delay_us(1);
            r.pwm_startclrcnt_en0()
                .modify(|_, w| w.pwm_startclrcnt_en_0().clear_bit());
            unsafe {
                r.pwm_start0()
                    .write_with_zero(|w| w.pwm_start_0().set_bit());
            }
            wait.delay_us(10);
            let staged = r.pwm_periodload_flag0().read().bits();
            #[cfg(feature = "pwm0-finite-counter-probe")]
            {
                // Collect before printing: UART timing must not shift observations.
                let mut samples = [(0u32, 0u16, 0u16); 9];
                for (slot, us) in samples
                    .iter_mut()
                    .zip([40, 90, 140, 240, 440, 740, 840, 1540, 1740])
                {
                    for _ in 0..1_000_000 {
                        let elapsed = tcxo.borrow_mut().read_counter().unwrap().ticks() - origin;
                        if elapsed >= us * 32 {
                            *slot = (
                                elapsed as u32,
                                r.pwm_periodload_flag0().read().bits() as u16,
                                r.pwm_periodcnt0().read().bits() as u16,
                            );
                            break;
                        }
                    }
                    if slot.0 == 0 {
                        super::fail();
                    }
                }
                for (ticks, flag, count) in samples {
                    super::emit(uart, b"WS53 PWM COUNT SAMPLE ");
                    hex(uart, (ticks >> 16) as u16);
                    hex(uart, ticks as u16);
                    hex(uart, flag);
                    hex(uart, count);
                    super::emit(uart, b"\r\n");
                }
            }
            super::emit(uart, b"WS53 PWM LOAD SAMPLE ");
            for value in [before, loaded, count, cleared, staged] {
                hex(uart, value as u16);
            }
            super::emit(uart, b"\r\n");
            0
        };
        #[cfg(feature = "pwm0-finite-irq-probe")]
        irq::after_start(&pwm, phase);
        super::emit(uart, b"WS53 PWM FINITE START ");
        super::emit(uart, &[b'0' + outcome]);
        super::emit(uart, b"\r\n");
        // A clock-based5ms observation closes the virtual-UART timing gap.
        // Host additionally holds the stage for40ms to expose repeated output.
        let mut wait = delay;
        wait.delay_ms(5);
        stage(uart, phase as u8 * 2 + 1);
        #[cfg(not(feature = "pwm0-finite-irq-probe"))]
        let seen = pwm.events();
        #[cfg(feature = "pwm0-finite-irq-probe")]
        let seen = irq::observe(&mut pwm, &mut wait, uart, phase);
        super::emit(uart, b"WS53 PWM FINITE EVENTS ");
        hex(uart, seen.group_errors);
        hex(uart, seen.count_errors);
        hex(uart, seen.configuration_loaded.into());
        super::emit(uart, b"\r\n");
        good &= outcome == 0
            && seen.group_errors == 0
            && seen.count_errors == 0
            && seen.configuration_loaded == 1;
        #[cfg(not(feature = "pwm0-finite-irq-probe"))]
        {
            good &= pwm.events() == seen;
            // Both PWM local sources remain disabled through this polling phase.
            let ack = unsafe { pwm::acknowledge_interrupt() };
            good &= ack == seen && pwm.events() == pwm::Events::default();
        }
        pwm.stop(Group::G0);
    }
    pwm.clear_group(Group::G0);
    let mut owner = pwm.detach(Channel::C0).unwrap().unwrap().into_pad();
    owner.configure(original).unwrap();
    good &= owner.configuration().unwrap() == original;
    drop(pwm);
    if good {
        #[cfg(not(any(feature = "pwm0-finite-irq-probe", feature = "pwm0-abnormal-probe")))]
        super::emit(uart, b"WS53 PASS pwm_finite bursts=2 poll=1 restore=1\r\n");
        #[cfg(feature = "pwm0-abnormal-probe")]
        super::emit(
            uart,
            b"WS53 PASS pwm_fault irq=71 faults=2 recovery=8,16 restore=1\r\n",
        );
        #[cfg(feature = "pwm0-finite-irq-probe")]
        super::emit(
            uart,
            b"WS53 PASS pwm_irq bursts=2 irq=72 notifications=2 empty=1 mask=1 rearm=1 restore=1\r\n",
        );
    } else {
        super::emit(uart, b"WS53 FAIL pwm_finite observed\r\n");
    }
}
