//! QEMU-only PWM fixture: synthetic pad permissions, no physical route proof.
#![no_std]
#![no_main]
use core::cell::{Cell, RefCell};
use critical_section::Mutex;
use embedded_hal::{delay::DelayNs, pwm::SetDutyCycle};
use hisi_hal::peripherals::Uart2;
use hisi_hal::pinctrl::PadControl;
use hisi_hal::pwm::{
    self, Channel, Error, EventEnable, Group, GroupConfig, OutputRoute, PulseCount, PulseMode,
    PwmDriver, Waveform,
};
use hisi_hal::soc::ws53::pins::Pad;
use hisi_hal::timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks};
use hisi_hal::uart::Uart;
use hisi_hal::{Peripherals, interrupt};
use hisi_riscv_rt::entry;
use hisi_riscv_rt::interrupt::ExternalInterrupt;

static ENDS: Mutex<Cell<u8>> = Mutex::new(Cell::new(0));
fn record() {
    let seen = unsafe { pwm::acknowledge_interrupt() };
    assert_eq!((seen.group_errors, seen.count_errors), (0, 0));
    critical_section::with(|cs| {
        let ends = ENDS.borrow(cs);
        ends.set(ends.get() | seen.configuration_loaded);
    });
}
#[unsafe(no_mangle)]
extern "C" fn PWM_CFG() {
    record();
}
#[unsafe(no_mangle)]
extern "C" fn PWM_ABNOR() {
    record();
}
fn ends() -> u8 {
    critical_section::with(|cs| ENDS.borrow(cs).get())
}
#[derive(Clone, Copy)]
struct TimerDelay<'a, 'd>(&'a RefCell<TimerDriver<'d>>);
impl DelayNs for TimerDelay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        let start = self
            .0
            .borrow_mut()
            .current_value(TimerChannel::Channel1)
            .unwrap();
        for _ in 0..10_000_000 {
            let now = self
                .0
                .borrow_mut()
                .current_value(TimerChannel::Channel1)
                .unwrap();
            if start.wrapping_sub(now) >= ticks {
                return;
            }
        }
        panic!("TIMER stopped");
    }
}
fn emit(uart: &Uart<'_, Uart2<'_>>, bytes: &[u8]) {
    for &byte in bytes {
        let mut remaining = 1_000_000;
        while uart.try_write_byte(byte).is_err() {
            remaining -= 1;
            assert!(remaining != 0, "UART stopped");
        }
    }
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    // Only the QEMU profile supplies these handoff assumptions. Never run this
    // fixture on a board: it establishes neither RAM ownership nor pad routing.
    let uart = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    let timer = RefCell::new(TimerDriver::new(unsafe {
        p.TIMER.assume_application_ownership()
    }));
    timer.borrow_mut().configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.borrow_mut().enable(TimerChannel::Channel1);
    let mut delay = TimerDelay(&timer);
    let mut pwm = PwmDriver::new(unsafe { p.PWM.assume_application_ownership() }, delay);
    let pads = [
        Pad::Mgpio3,
        Pad::Mgpio4,
        Pad::Mgpio5,
        Pad::Mgpio8,
        Pad::Mgpio9,
        Pad::Mgpio10,
        Pad::Mgpio12,
        Pad::Mgpio13,
    ];
    for (i, pad) in pads.into_iter().enumerate() {
        // Synthetic exclusive permissions only; QEMU observes channel outputs,
        // not these pad identifiers. No pinmux writes or silicon mapping claim.
        let route = unsafe {
            OutputRoute::from_configured(
                Channel::from_index(i as u8).unwrap(),
                PadControl::claim(pad).unwrap(),
            )
        };
        assert!(pwm.attach(route).is_ok());
    }
    emit(&uart, b"WS53 PWM QEMU begin\n");
    let wave = Waveform::new(
        150,
        100,
        10,
        PulseMode::Finite(PulseCount::new(2000).unwrap()),
    )
    .unwrap();
    for group in 0..4 {
        let a = Channel::from_index(group * 2).unwrap();
        let b = Channel::from_index(group * 2 + 1).unwrap();
        let g = Group::from_index(group).unwrap();
        pwm.configure_group(g, GroupConfig::new(&[(a, wave), (b, wave)]).unwrap())
            .unwrap();
        pwm.start(g).unwrap();
    }
    pwm.set_event_enable(EventEnable::All);
    interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
    unsafe {
        interrupt::enable(ExternalInterrupt::PWM_CFG);
        interrupt::enable(ExternalInterrupt::PWM_ABNOR);
        riscv::interrupt::enable();
    }
    delay.delay_ms(20);
    if cfg!(feature = "clock-stall-smoke") {
        assert_eq!(ends(), 0);
    } else {
        assert_eq!(ends(), 255);
    }
    for group in 0..4 {
        pwm.clear_group(Group::from_index(group).unwrap());
    }
    assert_eq!(pwm.events().configuration_loaded, 0);
    if !cfg!(feature = "clock-stall-smoke") {
        critical_section::with(|cs| ENDS.borrow(cs).set(0));
        let short = Waveform::new(8, 8, 0, PulseMode::Finite(PulseCount::new(1).unwrap())).unwrap();
        pwm.configure_group(
            Group::G0,
            GroupConfig::new(&[(Channel::C0, short)]).unwrap(),
        )
        .unwrap();
        assert_eq!(pwm.start(Group::G0), Err(Error::FiniteDeadlineMissed));
        delay.delay_ms(1);
        assert_eq!(ends(), 0); // stop/ack happened before either PWM IRQ resumed
        assert!(interrupt::is_enabled(ExternalInterrupt::PWM_CFG));
        assert!(riscv::register::mstatus::read().mie());
        pwm.clear_group(Group::G0);
    }
    let continuous = Waveform::new(150, 100, 0, PulseMode::Continuous).unwrap();
    pwm.configure_group(
        Group::G0,
        GroupConfig::new(&[(Channel::C0, continuous)]).unwrap(),
    )
    .unwrap();
    pwm.start(Group::G0).unwrap();
    {
        let mut channel = pwm.channel(Channel::C0).unwrap();
        assert_eq!(channel.max_duty_cycle(), 250);
        assert_eq!(channel.set_duty_cycle(251), Err(Error::InvalidDuty));
        channel.set_duty_cycle(125).unwrap();
        channel.set_duty_cycle_fully_off().unwrap();
        channel.set_duty_cycle_fully_on().unwrap();
    }
    pwm.clear_group(Group::G0);
    let pad = pwm.detach(Channel::C0).unwrap().unwrap().into_pad();
    assert_eq!(pad.pad(), Pad::Mgpio3);
    drop(pwm);
    assert!(!interrupt::is_enabled(ExternalInterrupt::PWM_CFG));
    assert!(!interrupt::is_enabled(ExternalInterrupt::PWM_ABNOR));
    if cfg!(feature = "clock-stall-smoke") {
        emit(&uart, b"WS53 PASS PWM clock_stall events=0 stop=1 drop=1\n");
    } else {
        emit(
            &uart,
            b"WS53 PASS PWM channels=8 groups=4 irq=72 late_stop=1 duty=1 drop=1\n",
        );
    }
    timer.borrow_mut().disable(TimerChannel::Channel1);
    emit(&uart, b"WS53 PWM QEMU done\n");
    loop {
        riscv::asm::wfi();
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
