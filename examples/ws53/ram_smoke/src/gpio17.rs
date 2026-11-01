//! Single-pad A-board fixture: MIO17/MGPIO17/SDK17 -> DSLogic D5.
//! Requires powered VDDIO, analyzer input only, old driver disabled, clocks
//! active and both vendor CPUs quiesced by the bounded RAM runner.
use hisi_hal::gpio::{GpioDriver, Pad};
use hisi_hal::peripherals::Uart2;
use hisi_hal::pinctrl::{Config, PadControl, Pull};
use hisi_hal::soc::ws53::pins::MuxMode;
use hisi_hal::uart::Uart;

fn configuration(pull: Pull) -> Config {
    let pad = Pad::Mgpio17;
    Config::new(
        pad.select_mode(MuxMode::from_code(0).unwrap()).unwrap(),
        pad.drive_setting(0).unwrap(),
        pull,
        true,
        true,
    )
    .unwrap()
}

fn stage(uart: &mut Uart<'_, Uart2<'_>>, number: u8) {
    super::emit(uart, b"WS53 GPIO17 STAGE ");
    super::emit(uart, &[b'0' + number]);
    super::emit(uart, b"\r\n");
    let mut budget = 100_000_000u32;
    loop {
        match uart.read_byte() {
            Ok(b'g') => return,
            Err(nb::Error::WouldBlock) => (),
            _ => super::fail(),
        }
        budget -= 1;
        if budget == 0 {
            super::fail();
        }
    }
}

pub fn check(gpio: &mut GpioDriver<'_>, uart: &mut Uart<'_, Uart2<'_>>) {
    let mut owner = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    let original = owner.configuration().unwrap();
    let cfg = configuration(Pull::None);
    owner.configure(cfg).unwrap();
    if owner.configuration().unwrap() != cfg {
        super::fail();
    }
    let pin = owner.into_gpio().unwrap_or_else(|_| super::fail());
    let mut output = gpio
        .init_output(pin, false)
        .unwrap_or_else(|_| super::fail());
    for (step, high) in [false, true, false, true, false, true]
        .into_iter()
        .enumerate()
    {
        if high {
            output.set_high();
        } else {
            output.set_low();
        }
        if output.is_set_high() != high {
            super::fail();
        }
        stage(uart, step as u8);
    }
    // Release before reclaiming the same physical pad for pull changes.
    let _ = output.free();
    for (step, pull) in [Pull::Down, Pull::Up].into_iter().enumerate() {
        let mut owner = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
        let cfg = configuration(pull);
        owner.configure(cfg).unwrap();
        if owner.configuration().unwrap() != cfg {
            super::fail();
        }
        let pin = owner.into_gpio().unwrap_or_else(|_| super::fail());
        let input = gpio.init_input(pin).unwrap_or_else(|_| super::fail());
        stage(uart, 6 + step as u8);
        if input.is_high() != (pull == Pull::Up) {
            super::fail();
        }
        let _ = input.free();
    }
    let mut owner = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    owner.configure(original).unwrap();
    if owner.configuration().unwrap() != original {
        super::fail();
    }
    // Output is disabled and pad configuration restored. Latch/IRQ state is
    // not restored; the runner resets the whole board, never resumes old RAM.
    super::emit(uart, b"WS53 PASS gpio17 output=6 input=2 pad_restore=1\r\n");
}
