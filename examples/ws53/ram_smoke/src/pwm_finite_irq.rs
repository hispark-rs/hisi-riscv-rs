//! IRQ72 delivery, peripheral masking, acknowledgement and rearm on PWM0.
use core::cell::Cell;
use critical_section::Mutex;
use embedded_hal::delay::DelayNs;
use hisi_hal::interrupt;
use hisi_hal::peripherals::Uart2;
use hisi_hal::pwm::{self, EventEnable, Events, PwmDriver};
use hisi_hal::uart::Uart;
use hisi_riscv_rt::interrupt::ExternalInterrupt;

static COUNT: Mutex<Cell<u16>> = Mutex::new(Cell::new(0));
static EMPTY: Mutex<Cell<u16>> = Mutex::new(Cell::new(0));
static INITIAL_PENDING: Mutex<Cell<(u16, u16)>> = Mutex::new(Cell::new((0, 0)));
static SEEN: Mutex<Cell<Events>> = Mutex::new(Cell::new(Events {
    group_errors: 0,
    count_errors: 0,
    configuration_loaded: 0,
}));
#[unsafe(no_mangle)]
extern "C" fn PWM_CFG() {
    // Own the complete block; the ISR only acknowledges and records.
    let seen = unsafe { pwm::acknowledge_interrupt() };
    critical_section::with(|cs| {
        let count = COUNT.borrow(cs);
        count.set(count.get().saturating_add(1));
        if seen == Events::default() {
            let empty = EMPTY.borrow(cs);
            empty.set(empty.get().saturating_add(1));
        }
        let old = SEEN.borrow(cs).get();
        SEEN.borrow(cs).set(Events {
            group_errors: old.group_errors | seen.group_errors,
            count_errors: old.count_errors | seen.count_errors,
            configuration_loaded: old.configuration_loaded | seen.configuration_loaded,
        });
    });
}
fn snapshot() -> (u16, Events) {
    critical_section::with(|cs| (COUNT.borrow(cs).get(), SEEN.borrow(cs).get()))
}
pub fn arm<D: DelayNs>(pwm: &mut PwmDriver<'_, D>, phase: usize) {
    // The foundation TIMER test leaves global delivery disabled.
    assert!(!riscv::register::mstatus::read().mie());
    assert_eq!(snapshot().0, phase as u16);
    assert!(!interrupt::is_enabled(ExternalInterrupt::PWM_CFG));
    assert!(!interrupt::is_enabled(ExternalInterrupt::PWM_ABNOR));
    unsafe {
        pwm::acknowledge_interrupt();
    }
    pwm.set_event_enable(if phase == 0 {
        EventEnable::Disabled
    } else {
        EventEnable::ConfigurationLoaded
    });
    interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
    unsafe {
        if phase == 0 {
            interrupt::enable(ExternalInterrupt::PWM_CFG);
        }
        riscv::interrupt::enable();
    }
    assert!(riscv::register::mstatus::read().mie());
}
pub fn after_start<D: DelayNs>(pwm: &PwmDriver<'_, D>, phase: usize) {
    if phase == 1 {
        // Read controller pending separately before permitting delivery.
        let pending = u16::from(interrupt::is_pending(ExternalInterrupt::PWM_CFG));
        let flag = u16::from(pwm.events().configuration_loaded);
        critical_section::with(|cs| INITIAL_PENDING.borrow(cs).set((pending, flag)));
        unsafe {
            interrupt::enable(ExternalInterrupt::PWM_CFG);
        }
    }
}
pub fn observe<D: DelayNs>(
    pwm: &mut PwmDriver<'_, D>,
    wait: &mut impl DelayNs,
    uart: &mut Uart<'_, Uart2<'_>>,
    phase: usize,
) -> Events {
    if phase == 0 {
        // Peripheral mask must retain raw status without delivering IRQ72.
        assert_eq!(snapshot().0, 0);
        assert_eq!(
            pwm.events(),
            Events {
                configuration_loaded: 1,
                ..Events::default()
            }
        );
        pwm.set_event_enable(EventEnable::ConfigurationLoaded);
        wait.delay_us(100);
    }
    let (count, seen) = snapshot();
    let raw = pwm.events();
    let (pending, initial_flag) = critical_section::with(|cs| INITIAL_PENDING.borrow(cs).get());
    super::super::emit(uart, b"WS53 PWM IRQ STATE ");
    for value in [
        count,
        critical_section::with(|cs| EMPTY.borrow(cs).get()),
        seen.configuration_loaded.into(),
        raw.configuration_loaded.into(),
        pending,
        initial_flag,
    ] {
        super::hex(uart, value);
    }
    super::super::emit(uart, b"\r\n");
    assert_eq!(count, if phase == 0 { 1 } else { 3 });
    assert_eq!(
        critical_section::with(|cs| EMPTY.borrow(cs).get()),
        phase as u16
    );
    assert_eq!((pending, initial_flag), (phase as u16, 0));
    assert_eq!(
        seen,
        Events {
            configuration_loaded: 1,
            ..Events::default()
        }
    );
    assert_eq!(pwm.events(), Events::default());
    wait.delay_us(100);
    assert_eq!(snapshot(), (count, seen)); // no unacknowledged level interrupt storm
    super::super::emit(uart, b"WS53 PWM IRQ ");
    super::hex(uart, count);
    super::super::emit(uart, b" mask=1 clear=1\r\n");
    pwm.set_event_enable(EventEnable::Disabled);
    unsafe {
        interrupt::disable(ExternalInterrupt::PWM_CFG);
    }
    riscv::interrupt::disable();
    assert!(!interrupt::is_enabled(ExternalInterrupt::PWM_ABNOR));
    seen
}
