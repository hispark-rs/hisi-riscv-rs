//! A-board PWM0P on MIO21/SGPIO2/SDK47, mode1, DSLogic D11.
//! SDK PWM sample and WS53 hardware guide agree on this route. Other PWM0P
//! aliases must not select PWM; all PWM users/IRQs/DMA must be quiesced, high
//! count halves zero, VDDIO powered and the only load an analyzer input.
use embedded_hal::delay::DelayNs;
#[cfg(not(feature = "pwm0-clock-probe"))]
use embedded_hal::pwm::SetDutyCycle;
use hisi_hal::peripherals::{Pwm, Uart2};
use hisi_hal::pinctrl::{Config, PadControl, Pull};
#[cfg(feature = "pwm0-clock-probe")]
use hisi_hal::pwm::ClockDivider;
use hisi_hal::pwm::{Channel, Group, GroupConfig, OutputRoute, PulseMode, PwmDriver, Waveform};
use hisi_hal::soc::ws53::pins::{MuxMode, Pad};
use hisi_hal::tcxo::TcxoDriver;
use hisi_hal::uart::Uart;

struct TcxoDelay<'d>(TcxoDriver<'d>);
impl DelayNs for TcxoDelay<'_> {
    fn delay_ns(&mut self, ns: u32) {
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000);
        let first = self.0.read_counter().unwrap().ticks();
        for _ in 0..1_000_000 {
            let now = self.0.read_counter().unwrap().ticks();
            if now.checked_sub(first).unwrap() >= ticks {
                return;
            }
        }
        super::fail();
    }
}
#[allow(dead_code)] // Flash uses a timer-driven stage callback instead.
fn stage(uart: &mut Uart<'_, Uart2<'_>>, number: u8) {
    #[cfg(not(feature = "pwm0-clock-probe"))]
    super::emit(uart, b"WS53 PWM0 STAGE ");
    #[cfg(feature = "pwm0-clock-probe")]
    super::emit(uart, b"WS53 PWM CLOCK STAGE ");
    super::emit(uart, &[b'0' + number]);
    super::emit(uart, b"\r\n");
    let mut budget = 100_000_000u32;
    loop {
        match uart.read_byte() {
            Ok(b'g') => return,
            Err(nb::Error::WouldBlock) => (),
            _ => super::fail(),
        }
        if budget == 0 {
            super::fail();
        }
        budget -= 1;
    }
}
#[allow(dead_code)] // Preserve the host-paced RAM fixture entry point.
pub fn check(token: Pwm<'_>, tcxo: TcxoDriver<'_>, uart: &mut Uart<'_, Uart2<'_>>) {
    check_with_stage(token, tcxo, uart, stage);
}

pub fn check_with_stage(
    token: Pwm<'_>,
    mut tcxo: TcxoDriver<'_>,
    uart: &mut Uart<'_, Uart2<'_>>,
    mut stage: impl FnMut(&mut Uart<'_, Uart2<'_>>, u8),
) {
    tcxo.enable().unwrap(); // Delay only; preserve the inherited counter epoch.
    let mut pwm = PwmDriver::new(token, TcxoDelay(tcxo));
    // Read only the existing PAC clock fields; no new clock-tree writes.
    // Divider16 does not by itself establish the SDK-reported 32MHz rate.
    let clocks = unsafe { &*ws53_pac::MCrg::PTR };
    let div = clocks.pwm_m_div3().read();
    if div.divider().bits() != 16
        || !div.enable().bit_is_set()
        || !div.load().bit_is_set()
        || !clocks.m_clken0().read().pwm_enable().bit_is_set()
    {
        super::fail();
    }
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
    // Verified sample pin47/mode1/channel0; HIL rejects conflicting aliases.
    let route = unsafe { OutputRoute::from_configured(Channel::C0, owner) };
    if pwm.attach(route).is_err() {
        super::fail();
    }
    let wave = Waveform::new(150, 50, 0, PulseMode::Continuous).unwrap();
    pwm.configure_group(Group::G0, GroupConfig::new(&[(Channel::C0, wave)]).unwrap())
        .unwrap();
    #[cfg(not(feature = "pwm0-clock-probe"))]
    {
        pwm.start(Group::G0).unwrap();
        stage(uart, 0);
        for (index, high) in [100, 150, 0, 200].into_iter().enumerate() {
            let mut channel = pwm.channel(Channel::C0).unwrap();
            if channel.max_duty_cycle() != 200 {
                super::fail();
            }
            channel.set_duty_cycle(high).unwrap();
            stage(uart, index as u8 + 1);
        }
        pwm.stop(Group::G0);
        stage(uart, 5);
    }
    #[cfg(feature = "pwm0-clock-probe")]
    {
        // Diagnostic only: whole PWM block is quiesced and exclusively handed
        // over. Change only its existing divider field while G0 is stopped;
        // do not modify upstream source, PLL or other peripheral clock fields.
        for (index, divider) in [16u8, 8, 4, 2, 1].into_iter().enumerate() {
            pwm.stop(Group::G0);
            pwm.set_clock_divider(ClockDivider::from_divisor(divider).unwrap())
                .unwrap();
            let readback = clocks.pwm_m_div3().read();
            if readback.divider().bits() != divider
                || !readback.enable().bit_is_set()
                || !readback.load().bit_is_set()
            {
                super::fail();
            }
            pwm.start(Group::G0).unwrap();
            if pwm.set_clock_divider(ClockDivider::Div16) != Err(hisi_hal::pwm::Error::GroupRunning)
            {
                super::fail();
            }
            stage(uart, index as u8);
        }
        pwm.stop(Group::G0);
        stage(uart, 5);
        pwm.set_clock_divider(ClockDivider::Div16).unwrap();
        if clocks.pwm_m_div3().read().divider().bits() != 16 {
            super::fail();
        }
    }
    let events = pwm.events();
    if events.group_errors != 0 || events.count_errors != 0 || events.configuration_loaded != 0 {
        super::fail();
    }
    pwm.clear_group(Group::G0);
    let mut owner = pwm.detach(Channel::C0).unwrap().unwrap().into_pad();
    owner.configure(original).unwrap();
    if owner.configuration().unwrap() != original {
        super::fail();
    }
    drop(pwm);
    #[cfg(not(feature = "pwm0-clock-probe"))]
    super::emit(uart, b"WS53 PASS pwm0 pad=47 stages=6 restore=1\r\n");
    #[cfg(feature = "pwm0-clock-probe")]
    super::emit(uart, b"WS53 PASS pwm_clock pad=47 dividers=5 restore=1\r\n");
}
