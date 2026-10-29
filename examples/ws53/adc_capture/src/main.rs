//! QEMU-only capture/retention fixture. Never load on physical hardware.
#![no_std]
#![no_main]
use core::{cell::RefCell, num::NonZeroU16};
use embedded_hal::delay::DelayNs;
use hisi_hal::Peripherals;
use hisi_hal::adc::{
    Calibrator, Channel, Sample,
    capture::{Buffer, Capture, Error, Input, Inputs},
};
use hisi_hal::peripherals::Uart2;
use hisi_hal::pinctrl::PadControl;
use hisi_hal::soc::ws53::pins::Pad;
use hisi_hal::timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks};
use hisi_hal::uart::Uart;
use hisi_riscv_rt::entry;

// Symbols and guard values are independently inspected by the host, not MMIO.
#[unsafe(no_mangle)]
static mut WS53_ADC_CAPTURE_WORDS: [u32; 11] = [0xdeadbeef; 11];
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
            assert!(remaining != 0);
        }
    }
}
fn receive(uart: &mut Uart<'_, Uart2<'_>>) -> u8 {
    for _ in 0..100_000_000 {
        if let Ok(byte) = uart.read_byte() {
            return byte;
        }
    }
    panic!("host handshake timed out");
}
unsafe fn route(pad: Pad) -> Input<'static> {
    // Test-only profile has no physical analog pads or competing owners.
    unsafe {
        Input::from_configured(PadControl::claim(pad).unwrap())
            .ok()
            .unwrap()
    }
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    // These external handoffs are true only in this isolated explicit QEMU model.
    let mut uart = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
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
    let mut adc = Calibrator::new(unsafe { p.ADC.assume_application_ownership() });
    adc.calibrate(NonZeroU16::new(3).unwrap(), &mut delay)
        .unwrap();
    emit(&uart, b"WS53 CAPTURE begin\n");
    // N=normal 9 words; E=early-DONE stalled final write; T=bus timeout;
    // B=bus error; S=sample stall; D=drop; F=forget. All faults are host injected.
    let scenario = receive(&mut uart);
    let length = if scenario == b'N' { 9 } else { 1 };
    let mut inputs = Inputs::new(unsafe { route(Pad::Agpio1) });
    assert!(inputs.add(unsafe { route(Pad::Agpio6) }).is_ok());
    assert!(inputs.add(unsafe { route(Pad::Agpio0) }).is_ok());
    let buffer = unsafe {
        // QEMU maps coherent RAM, without caches or any other writer. No such
        // claim has been established for the installed board's memory mapping.
        let start = core::ptr::addr_of_mut!(WS53_ADC_CAPTURE_WORDS)
            .cast::<u32>()
            .add(1);
        Buffer::from_coherent_static(core::slice::from_raw_parts_mut(start, length))
            .ok()
            .unwrap()
    };
    let mut capture = unsafe {
        Capture::from_calibrated(
            adc,
            p.MCU_DIAG.assume_application_ownership(),
            inputs,
            buffer,
        )
    };
    capture.start().unwrap();
    if scenario == b'D' || scenario == b'F' {
        delay.delay_us(20);
        if scenario == b'D' {
            drop(capture);
        } else {
            core::mem::forget(capture);
        }
        emit(&uart, b"WS53 CAPTURE abandoned\n");
        // Never recreate a reference to the abandoned allocation.
        assert_eq!(receive(&mut uart), b'R');
        emit(&uart, b"WS53 PASS CAPTURE static_retained=1\n");
    } else {
        let completion = capture.wait(NonZeroU16::new(1000).unwrap(), &mut delay);
        if scenario == b'N' || scenario == b'E' {
            assert!(completion.is_ok());
            emit(&uart, b"WS53 CAPTURE observed\n");
        } else {
            assert_eq!(completion, Err(Error::Timeout));
            emit(&uart, b"WS53 CAPTURE timeout\n");
        }
        assert_eq!(capture.start(), Err(Error::NotReady));
        // Host must prove producer disabled AND QOM pending writes exhausted,
        // then send R. This is an external model proof, not a silicon drain API.
        assert_eq!(receive(&mut uart), b'R');
        let (adc, _diag, _inputs, buffer) = unsafe { capture.release_after_external_quiescence() };
        assert!(adc.result().is_none());
        let words = buffer.into_words();
        for (i, &word) in words.iter().enumerate() {
            if scenario == b'B' || scenario == b'S' {
                assert_eq!(word, 0xdeadbeef);
            } else {
                let channel = [Channel::Channel0, Channel::Channel3, Channel::Channel7][i % 3];
                let sample = Sample::from_capture_word(word);
                assert_eq!(sample.channel(), channel);
                assert_eq!(sample.code(), [100, 400, 800][i % 3]);
            }
        }
        emit(
            &uart,
            b"WS53 PASS CAPTURE external_drain=1 data=1 calibration_invalidated=1\n",
        );
    }
    emit(&uart, b"WS53 CAPTURE done\n");
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
