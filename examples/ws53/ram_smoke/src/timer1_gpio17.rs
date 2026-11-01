//! A-board TIMER1 IRQ54 and timing fixture. Only drives MIO17/D5.
//! Same ownership, power and input-only wiring prerequisites as gpio17-smoke.
use core::cell::Cell;
use critical_section::Mutex;
use hisi_hal::gpio::{GpioDriver, Pad};
use hisi_hal::interrupt;
use hisi_hal::peripherals::Uart2;
use hisi_hal::pinctrl::{Config, PadControl, Pull};
use hisi_hal::soc::ws53::pins::MuxMode;
use hisi_hal::timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks};
use hisi_hal::uart::Uart;

const CHANNEL: TimerChannel = TimerChannel::Channel1;
static NOTIFICATIONS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));

fn count() -> u32 {
    critical_section::with(|cs| NOTIFICATIONS.borrow(cs).get())
}

fn wait_for(expected: u32) {
    let mut budget = 100_000_000u32;
    loop {
        let got = count();
        if got == expected {
            return;
        }
        if got > expected || budget == 0 {
            super::fail();
        }
        budget -= 1;
        core::hint::spin_loop();
    }
}

fn stage(uart: &mut Uart<'_, Uart2<'_>>, number: u8) {
    super::emit(uart, b"WS53 TIMER1 GPIO17 STAGE ");
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

fn start(timer: &mut TimerDriver<'_>) {
    unsafe {
        interrupt::enable(CHANNEL.interrupt());
        riscv::interrupt::enable();
    }
    timer.enable(CHANNEL);
}

fn stop(timer: &mut TimerDriver<'_>) {
    riscv::interrupt::disable();
    unsafe {
        interrupt::disable(CHANNEL.interrupt());
    }
    timer.disable(CHANNEL);
    timer.clear_interrupt(CHANNEL);
    if timer.interrupt_pending(CHANNEL) {
        super::fail();
    }
}

pub fn check(
    gpio: &mut GpioDriver<'_>,
    timer: &mut TimerDriver<'_>,
    uart: &mut Uart<'_, Uart2<'_>>,
) {
    let mut owner = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    let original = owner.configuration().unwrap();
    let pad = Pad::Mgpio17;
    let cfg = Config::new(
        pad.select_mode(MuxMode::from_code(0).unwrap()).unwrap(),
        pad.drive_setting(0).unwrap(),
        Pull::None,
        true,
        true,
    )
    .unwrap();
    owner.configure(cfg).unwrap();
    if owner.configuration().unwrap() != cfg || count() != 0 {
        super::fail();
    }
    let pin = owner.into_gpio().unwrap_or_else(|_| super::fail());
    let mut output = gpio
        .init_output(pin, false)
        .unwrap_or_else(|_| super::fail());
    for (phase, us) in [5_000, 10_000].into_iter().enumerate() {
        timer.configure(
            CHANNEL,
            TimerMode::Periodic,
            TimerTicks::from_micros(us).unwrap(),
        );
        stage(uart, phase as u8 * 2);
        start(timer);
        for edge in 1..=16 {
            wait_for(phase as u32 * 16 + edge);
            // The handler only acknowledges and records; physical output stays
            // in main context. Measured periods include this short latency.
            if edge % 2 == 1 {
                output.set_high();
            } else {
                output.set_low();
            }
            if output.is_set_high() != (edge % 2 == 1) {
                super::fail();
            }
        }
        stop(timer);
        stage(uart, phase as u8 * 2 + 1);
        if count() != (phase as u32 + 1) * 16 {
            super::fail();
        }
    }
    for shot in 0..2 {
        timer.configure(
            CHANNEL,
            TimerMode::OneShot,
            TimerTicks::from_micros(5_000).unwrap(),
        );
        start(timer);
        wait_for(33 + shot);
        // Leave IRQ delivery enabled throughout the host's >=40ms dwell:
        // a periodic/repeating one-shot is detected, not hidden by masking.
        stage(uart, 4 + shot as u8);
        if count() != 33 + shot || timer.interrupt_pending(CHANNEL) {
            super::fail();
        }
        stop(timer);
    }
    timer.configure(
        CHANNEL,
        TimerMode::Periodic,
        TimerTicks::from_micros(5_000).unwrap(),
    );
    timer.set_interrupt_mask(CHANNEL, true);
    start(timer);
    stage(uart, 6);
    if count() != 34 || !timer.interrupt_pending(CHANNEL) {
        super::fail();
    }
    timer.set_interrupt_mask(CHANNEL, false);
    wait_for(35);
    stop(timer);
    stage(uart, 7);
    if count() != 35 || timer.interrupt_pending(CHANNEL) {
        super::fail();
    }
    let _ = output.free();
    let mut owner = unsafe { PadControl::claim(pad) }.unwrap();
    owner.configure(original).unwrap();
    if owner.configuration().unwrap() != original {
        super::fail();
    }
    super::emit(
        uart,
        b"WS53 PASS timer1_irq=54 notifications=35 periods=2 oneshot=2 mask=1\r\n",
    );
}

#[unsafe(no_mangle)]
extern "C" fn TIMER_1() {
    unsafe {
        TimerDriver::acknowledge_interrupt(CHANNEL);
    }
    critical_section::with(|cs| {
        let value = NOTIFICATIONS.borrow(cs);
        value.set(value.get() + 1);
    });
}
