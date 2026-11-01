//! Read-only checks replacing the RAM runner's debugger-side PWM preflight.
//! These checks precede controller initialization, clock writes and pad routing.
pub fn idle_profile() -> bool {
    let clocks = unsafe { &*ws53_pac::MCrg::PTR };
    if !clocks.m_clken0().read().pwm_enable().bit_is_set() {
        // Do not inspect a potentially gated peripheral register interface.
        return false;
    }
    let mcu = unsafe { &*ws53_pac::PinmuxMcu::PTR };
    let aon = unsafe { &*ws53_pac::PinmuxAon::PTR };
    let gpio = unsafe { &*ws53_pac::Gpio2::PTR };
    if mcu.mgpio10_sel().read().bits() & 7 == 4
        || aon.agpio2_sel().read().bits() & 7 == 2
        || mcu.mgpio21_sel().read().bits() & 7 != 0
        || gpio.gpio_sw_oen().read().bits() & 4 == 0
    {
        return false;
    }
    let pwm = unsafe { &*ws53_pac::Pwm::PTR };
    if pwm.pwm_sel0().read().bits() != 0
        || pwm.pwm_sel1().read().bits() != 0
        || pwm.pwm_sel2().read().bits() != 0
        || pwm.pwm_sel3().read().bits() != 0
        || pwm.pwm_dma_en().read().bits() != 0
    {
        return false;
    }
    macro_rules! idle {
        ($en:ident, $offset:ident, $frequency:ident, $duty:ident) => {
            pwm.$en().read().bits() == 0
                && pwm.$offset().read().bits() == 0
                && pwm.$frequency().read().bits() == 0
                && pwm.$duty().read().bits() == 0
        };
    }
    idle!(pwm_en0, pwm_offset_h0, pwm_freq_h0, pwm_duty_h0)
        && idle!(pwm_en1, pwm_offset_h1, pwm_freq_h1, pwm_duty_h1)
        && idle!(pwm_en2, pwm_offset_h2, pwm_freq_h2, pwm_duty_h2)
        && idle!(pwm_en3, pwm_offset_h3, pwm_freq_h3, pwm_duty_h3)
        && idle!(pwm_en4, pwm_offset_h4, pwm_freq_h4, pwm_duty_h4)
        && idle!(pwm_en5, pwm_offset_h5, pwm_freq_h5, pwm_duty_h5)
        && idle!(pwm_en6, pwm_offset_h6, pwm_freq_h6, pwm_duty_h6)
        && idle!(pwm_en7, pwm_offset_h7, pwm_freq_h7, pwm_duty_h7)
}
