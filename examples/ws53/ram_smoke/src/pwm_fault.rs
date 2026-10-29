//! Intentional PAC-only invalid configurations; all output pads remain unrouted.
//! The caller owns the whole PWM block, disables both sources, and later tests
//! valid HAL output on MIO21. No flash/security/shared-control writes occur.
use core::cell::Cell;
use critical_section::Mutex;
use embedded_hal::delay::DelayNs;
use hisi_hal::peripherals::Uart2;
use hisi_hal::uart::Uart;
use hisi_hal::{
    interrupt,
    pwm::{self, EventEnable, Events, PwmDriver},
};
use hisi_riscv_rt::interrupt::ExternalInterrupt;

static COUNT: Mutex<Cell<u16>> = Mutex::new(Cell::new(0));
static SEEN: Mutex<Cell<Events>> = Mutex::new(Cell::new(Events {
    group_errors: 0,
    count_errors: 0,
    configuration_loaded: 0,
}));
#[unsafe(no_mangle)]
extern "C" fn PWM_ABNOR() {
    // A still-invalid configuration reasserts errors after acknowledgement.
    // This diagnostic owns both outputs and enabled abnormal events only.
    unsafe { &*ws53_pac::Pwm::PTR }
        .pwm_int_mask()
        .modify(|_, w| w.pwm_int_mask().disabled());
    let seen = unsafe { pwm::acknowledge_interrupt() };
    let count = critical_section::with(|cs| {
        let count = COUNT.borrow(cs).get().saturating_add(1);
        COUNT.borrow(cs).set(count);
        let old = SEEN.borrow(cs).get();
        SEEN.borrow(cs).set(Events {
            group_errors: old.group_errors | seen.group_errors,
            count_errors: old.count_errors | seen.count_errors,
            configuration_loaded: old.configuration_loaded | seen.configuration_loaded,
        });
        count
    });
    if count >= 8 {
        // Bound an unexpected persistent hardware error instead of starving recovery.
        unsafe { &*ws53_pac::Pwm::PTR }
            .pwm_int_mask()
            .modify(|_, w| w.pwm_int_mask().disabled());
        unsafe {
            interrupt::disable(ExternalInterrupt::PWM_ABNOR);
        }
    }
}
fn commit(group1: bool) {
    let r = unsafe { &*ws53_pac::Pwm::PTR };
    if group1 {
        r.pwm_startclrcnt_en1()
            .modify(|_, w| w.pwm_startclrcnt_en_1().set_bit());
        unsafe {
            r.pwm_start1()
                .write_with_zero(|w| w.pwm_start_1().set_bit());
        }
    } else {
        r.pwm_startclrcnt_en0()
            .modify(|_, w| w.pwm_startclrcnt_en_0().set_bit());
        unsafe {
            r.pwm_start0()
                .write_with_zero(|w| w.pwm_start_0().set_bit());
        }
    }
}
pub fn check<D: DelayNs>(
    pwm: &mut PwmDriver<'_, D>,
    mut wait: impl DelayNs,
    uart: &mut Uart<'_, Uart2<'_>>,
) -> bool {
    assert!(!riscv::register::mstatus::read().mie());
    assert!(!interrupt::is_enabled(ExternalInterrupt::PWM_ABNOR));
    assert!(!interrupt::is_enabled(ExternalInterrupt::PWM_CFG));
    let r = unsafe { &*ws53_pac::Pwm::PTR };
    let mut good = true;
    for phase in 0..2 {
        unsafe {
            pwm::acknowledge_interrupt();
        }
        critical_section::with(|cs| {
            COUNT.borrow(cs).set(0);
            SEEN.borrow(cs).set(Events::default());
        });
        // PAD route remains GPIO input. Only deliberate invalid register values
        // bypass typed Waveform/GroupConfig; the valid recovery uses the HAL.
        r.pwm_en0().modify(|_, w| w.pwm_en_0().clear_bit());
        r.pwm_offset_l0()
            .modify(|_, w| unsafe { w.pwm_offset_l_0().bits(0) });
        r.pwm_freq_l0()
            .modify(|_, w| unsafe { w.pwm_freq_l_0().bits(if phase == 0 { 20 } else { 200 }) });
        r.pwm_duty_l0()
            .modify(|_, w| unsafe { w.pwm_duty_l_0().bits(100) });
        r.pwm_period_val0()
            .modify(|_, w| unsafe { w.pwm_period_val_0().bits(0) });
        r.pwm_sel0().modify(|_, w| unsafe { w.pwm_sel_0().bits(1) });
        r.pwm_sel1()
            .modify(|_, w| unsafe { w.pwm_sel_1().bits(if phase == 0 { 0 } else { 1 }) });
        pwm.set_event_enable(EventEnable::Abnormal);
        interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
        unsafe {
            interrupt::enable(ExternalInterrupt::PWM_ABNOR);
            riscv::interrupt::enable();
        }
        r.pwm_en0().modify(|_, w| w.pwm_en_0().set_bit());
        commit(phase == 1);
        wait.delay_us(1000);
        pwm.set_event_enable(EventEnable::Disabled);
        unsafe {
            interrupt::disable(ExternalInterrupt::PWM_ABNOR);
        }
        riscv::interrupt::disable();
        let (count, seen) =
            critical_section::with(|cs| (COUNT.borrow(cs).get(), SEEN.borrow(cs).get()));
        let remaining = pwm.events();
        super::super::emit(uart, b"WS53 PWM FAULT ");
        super::hex(uart, phase);
        for v in [
            count,
            seen.group_errors,
            seen.count_errors,
            seen.configuration_loaded.into(),
            remaining.group_errors,
            remaining.count_errors,
        ] {
            super::hex(uart, v);
        }
        super::super::emit(uart, b"\r\n");
        good &= count == 1
            && (seen.group_errors, seen.count_errors) == if phase == 0 { (0, 1) } else { (1, 0) }
            && (remaining.group_errors, remaining.count_errors)
                == if phase == 0 { (0, 1) } else { (1, 0) };
        // Recover valid count fields and disable channel before clearing selection.
        r.pwm_en0().modify(|_, w| w.pwm_en_0().clear_bit());
        r.pwm_sel1().modify(|_, w| unsafe { w.pwm_sel_1().bits(0) });
        r.pwm_freq_l0()
            .modify(|_, w| unsafe { w.pwm_freq_l_0().bits(200) });
        commit(false);
        wait.delay_us(10);
        r.pwm_sel0().modify(|_, w| unsafe { w.pwm_sel_0().bits(0) });
        unsafe {
            pwm::acknowledge_interrupt();
        }
        let recovered = pwm.events();
        super::super::emit(uart, b"WS53 PWM FAULT RECOVERY ");
        for value in [
            recovered.group_errors,
            recovered.count_errors,
            recovered.configuration_loaded.into(),
        ] {
            super::hex(uart, value);
        }
        super::super::emit(uart, b"\r\n");
        good &= pwm.events() == Events::default();
    }
    good
}
