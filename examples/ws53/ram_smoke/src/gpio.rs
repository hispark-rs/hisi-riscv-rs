//! QEMU-only GPIO data/IRQ integration. No board pin/clock/pad assumptions.
use core::cell::Cell;
use critical_section::Mutex;
use embedded_hal::digital::{InputPin, OutputPin, StatefulOutputPin};
use hisi_hal::gpio::{AnyPin, GpioBank, GpioDriver, InterruptTrigger, Pad};
use hisi_hal::interrupt;
use hisi_hal::peripherals::Uart2;
use hisi_hal::pinctrl::{Config, Error as PadError, PadControl, Pull};
use hisi_hal::soc::ws53::pins::MuxMode;
use hisi_hal::uart::Uart;

static COUNT: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static BAD: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));

fn config(pad: Pad, pull: Pull, input: bool) -> Config {
    let drive = pad
        .drive_setting(15)
        .or_else(|| pad.drive_setting(3))
        .unwrap();
    Config::new(
        pad.select_mode(MuxMode::from_code(0).unwrap()).unwrap(),
        drive,
        pull,
        input,
        true,
    )
    .unwrap()
}

fn prepare(owner: &mut PadControl<'_>, pull: Pull, input: bool) {
    let cfg = config(owner.pad(), pull, input);
    owner.configure(cfg).unwrap();
    if owner.configuration().unwrap() != cfg {
        super::fail();
    }
}

/// QEMU-only sweep of modeled pads, never a physical-board scan. UART chardevs
/// are diagnostic channels here; alternate peripheral pad signals are not modeled.
pub fn check_pads(uart: &mut Uart<'_, Uart2<'_>>) {
    let mut checked = 0;
    for pad in Pad::ALL {
        if pad == Pad::Reset || pad.is_debug() {
            continue;
        }
        let mut owner = unsafe { PadControl::claim(pad) }.unwrap();
        prepare(&mut owner, Pull::Down, false);
        let mut owner = match owner.into_gpio() {
            Err((PadError::GpioNotPrepared, owner)) => owner,
            _ => super::fail(),
        };
        prepare(&mut owner, Pull::Up, true);
        super::emit(uart, b"WS53 PAD ");
        let id = pad.sdk_id().number();
        super::emit(uart, &[b'0' + id / 10, b'0' + id % 10]);
        super::emit(uart, b" READY\r\n");
        receive_go(uart);
        // The transfer consumes PadControl, so it cannot reconfigure a live AnyPin.
        let pin = owner.into_gpio().unwrap_or_else(|_| super::fail());
        if pin.pad() != pad {
            super::fail();
        }
        drop(pin);
        checked += 1;
    }
    if checked != 28 {
        super::fail();
    }
    super::emit(uart, b"WS53 PASS pinctrl_pads=28 readback=1 transfer=1\r\n");
}

fn configured_pin(pad: Pad) -> AnyPin<'static> {
    let mut owner = unsafe { PadControl::claim(pad) }.unwrap();
    prepare(&mut owner, Pull::None, true);
    owner.into_gpio().unwrap_or_else(|_| super::fail())
}

fn stage(uart: &mut Uart<'_, Uart2<'_>>, bank: u8, name: &[u8], step: u8) {
    super::emit(uart, b"WS53 GPIO ");
    super::emit(uart, &[bank]);
    super::emit(uart, b" ");
    super::emit(uart, name);
    super::emit(uart, b" ");
    super::emit(uart, &[b'0' + step]);
    super::emit(uart, b"\r\n");
    receive_go(uart);
}

fn receive_go(uart: &mut Uart<'_, Uart2<'_>>) {
    let mut budget = 100_000_000u32;
    loop {
        match uart.read_byte() {
            Ok(b'g') => break,
            Err(nb::Error::WouldBlock) => (),
            _ => super::fail(),
        }
        budget -= 1;
        if budget == 0 {
            super::fail();
        }
    }
}

pub fn check(gpio: &mut GpioDriver<'_>, pad: Pad, uart: &mut Uart<'_, Uart2<'_>>, id: u8) {
    let bank = gpio.bank();
    let pin = configured_pin(pad);
    let mut output = gpio
        .init_output(pin, true)
        .unwrap_or_else(|_| super::fail());
    if !StatefulOutputPin::is_set_high(&mut output).unwrap() {
        super::fail();
    }
    stage(uart, id, b"HIGH", 0);
    OutputPin::set_low(&mut output).unwrap();
    if !StatefulOutputPin::is_set_low(&mut output).unwrap() {
        super::fail();
    }
    stage(uart, id, b"LOW", 0);
    StatefulOutputPin::toggle(&mut output).unwrap();
    stage(uart, id, b"HIGH", 1);
    let pin = output.free();
    stage(uart, id, b"RELEASED", 0);
    let mut input = gpio.init_input(pin).unwrap_or_else(|_| super::fail());
    stage(uart, id, b"INPUT", 0);
    if !InputPin::is_high(&mut input).unwrap() {
        super::fail();
    }

    // Both-edge is exercised in both directions. Level triggers stay active
    // after ISR EOI; one-shot local IRQ masking must prevent an interrupt storm.
    let triggers = [
        InterruptTrigger::RisingEdge,
        InterruptTrigger::FallingEdge,
        InterruptTrigger::BothEdges,
        InterruptTrigger::BothEdges,
        InterruptTrigger::HighLevel,
        InterruptTrigger::LowLevel,
    ];
    for (step, trigger) in triggers.into_iter().enumerate() {
        stage(uart, id, b"BASE", step as u8);
        input.listen(trigger);
        critical_section::with(|cs| {
            COUNT.borrow(cs).set(0);
            BAD.borrow(cs).set(false);
        });
        unsafe {
            interrupt::enable(bank.interrupt());
            riscv::interrupt::enable();
        }
        stage(uart, id, b"ARM", step as u8);
        // The runner supplies the external transition before its UART response.
        let mut budget = 1_000_000;
        while critical_section::with(|cs| COUNT.borrow(cs).get()) == 0 {
            budget -= 1;
            if budget == 0 {
                super::fail();
            }
        }
        riscv::interrupt::disable();
        if critical_section::with(|cs| COUNT.borrow(cs).get() != 1 || BAD.borrow(cs).get()) {
            super::fail();
        }
        if InputPin::is_high(&mut input).unwrap() != (step % 2 == 0) {
            super::fail();
        }
        input.unlisten();
    }
    // Drop/free must cancel the peripheral source AND the local pending latch.
    input.listen(InterruptTrigger::BothEdges);
    stage(uart, id, b"PENDING", 0);
    let pin = input.free();
    unsafe {
        interrupt::enable(bank.interrupt());
        riscv::interrupt::enable();
    }
    stage(uart, id, b"CANCEL", 0);
    riscv::interrupt::disable();
    if critical_section::with(|cs| COUNT.borrow(cs).get()) != 1 {
        super::fail();
    }
    unsafe {
        interrupt::disable(bank.interrupt());
    }
    // Exercise destructor paths too; output Drop must release drive.
    drop(
        gpio.init_output(pin, false)
            .unwrap_or_else(|_| super::fail()),
    );
    stage(uart, id, b"DROPPED", 0);
    // All previous tokens are gone; reclaim the isolated model pad for Drop.
    let pin = configured_pin(pad);
    let mut input = gpio.init_input(pin).unwrap_or_else(|_| super::fail());
    input.listen(InterruptTrigger::BothEdges);
    stage(uart, id, b"PENDING_DROP", 0);
    drop(input);
    unsafe {
        interrupt::enable(bank.interrupt());
        riscv::interrupt::enable();
    }
    stage(uart, id, b"DROPPED_INPUT", 0);
    riscv::interrupt::disable();
    if critical_section::with(|cs| COUNT.borrow(cs).get()) != 1 {
        super::fail();
    }
    unsafe {
        interrupt::disable(bank.interrupt());
    }
}

fn notify(bank: GpioBank, expected: Pad) {
    let status = unsafe { GpioDriver::on_interrupt(bank) };
    critical_section::with(|cs| {
        COUNT.borrow(cs).set(COUNT.borrow(cs).get() + 1);
        if status.is_empty() || !status.contains(expected) {
            BAD.borrow(cs).set(true);
        }
    });
}
#[unsafe(no_mangle)]
extern "C" fn GPIO_0() {
    notify(GpioBank::Bank0, Pad::Mgpio2);
}
#[unsafe(no_mangle)]
extern "C" fn GPIO_1() {
    notify(GpioBank::Bank1, Pad::Agpio3);
}
#[unsafe(no_mangle)]
extern "C" fn SGPIO_INT() {
    notify(GpioBank::Bank2, Pad::Sgpio2);
}
