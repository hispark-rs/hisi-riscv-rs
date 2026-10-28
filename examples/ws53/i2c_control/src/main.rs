//! QEMU-only I2C fixture. `wire-pads` selects real muxes with virtual peers;
//! it is still not a physical board takeover or an external-device test.
#![no_std]
#![no_main]
use core::{cell::RefCell, num::NonZeroU16};
use embedded_hal::{
    delay::DelayNs,
    i2c::{Error as _, ErrorKind, NoAcknowledgeSource, Operation},
};
use hisi_hal::i2c::{Address, Config, Error, Failure, I2c, Pins, Timing, Wait};
use hisi_hal::peripherals::Uart2;
use hisi_hal::pinctrl::PadControl;
use hisi_hal::soc::ws53::pins::Pad;
use hisi_hal::timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks};
use hisi_hal::uart::Uart;
use hisi_hal::{Peripherals, interrupt};
#[cfg(feature = "wire-pads")]
use hisi_hal::{
    pinctrl::{Config as PadConfig, Pull},
    soc::ws53::pins::MuxMode,
};
use hisi_riscv_rt::{entry, interrupt::ExternalInterrupt};

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
#[cfg(feature = "wire-pads")]
fn wire_pin(pad: Pad) -> PadControl<'static> {
    let mut owner = unsafe { PadControl::claim(pad) }.unwrap();
    let mode = pad.select_mode(MuxMode::from_code(3).unwrap()).unwrap();
    owner
        .configure(
            PadConfig::new(mode, pad.drive_setting(0).unwrap(), Pull::Up, true, false).unwrap(),
        )
        .unwrap();
    owner
}
fn exercise<T, D: DelayNs>(bus: &mut I2c<'_, T, D>) {
    for address in [Address::seven(0x50).unwrap(), Address::ten(0x250).unwrap()] {
        bus.transaction(
            address,
            &mut [
                Operation::Write(&[0x80, 0x12]),
                Operation::Write(&[0x34, 0x56]),
            ],
        )
        .unwrap();
        let mut a = [0; 1];
        let mut b = [0; 2];
        bus.transaction(
            address,
            &mut [
                Operation::Write(&[]),
                Operation::Write(&[0x80]),
                Operation::Read(&mut a),
                Operation::Read(&mut []),
                Operation::Read(&mut b),
            ],
        )
        .unwrap();
        assert_eq!(a, [0x12]);
        assert_eq!(b, [0x34, 0x56]);
        let mut long = [0u8; 100];
        bus.transaction(
            address,
            &mut [Operation::Write(&[0]), Operation::Read(&mut long)],
        )
        .unwrap();
        for (i, byte) in long.iter().enumerate() {
            assert_eq!(*byte, i as u8 ^ 0xa5);
        }
        bus.transaction(
            address,
            &mut [Operation::Read(&mut a), Operation::Write(&[0x80, 0x12])],
        )
        .unwrap();
        bus.transaction(address, &mut []).unwrap();
    }
    for address in [Address::seven(0x51).unwrap(), Address::ten(0x251).unwrap()] {
        let error = bus
            .transaction(address, &mut [Operation::Write(&[0])])
            .unwrap_err();
        assert_eq!(
            error.kind(),
            ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address)
        );
        assert!(!bus.is_poisoned());
        bus.transaction(Address::seven(0x50).unwrap(), &mut [Operation::Write(&[0])])
            .unwrap();
    }
    bus.recover().unwrap();
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    // QEMU supplies ownership and the clock profile. No physical board takeover,
    // pinmux writes, wire scans or Flash access are performed by this fixture.
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
    let delay = TimerDelay(&timer);
    let config = Config::new(
        Timing::from_hz(100_000).unwrap(),
        NonZeroU16::new(1000).unwrap(),
    );
    #[cfg(not(feature = "wire-pads"))]
    let pins0 = unsafe {
        Pins::from_configured(
            PadControl::claim(Pad::Mgpio3).unwrap(),
            PadControl::claim(Pad::Mgpio4).unwrap(),
        )
    };
    #[cfg(not(feature = "wire-pads"))]
    let pins1 = unsafe {
        Pins::from_configured(
            PadControl::claim(Pad::Mgpio5).unwrap(),
            PadControl::claim(Pad::Mgpio8).unwrap(),
        )
    };
    #[cfg(feature = "wire-pads")]
    let pins0 = unsafe { Pins::from_configured(wire_pin(Pad::Sgpio2), wire_pin(Pad::Mgpio22)) };
    #[cfg(feature = "wire-pads")]
    let pins1 = unsafe { Pins::from_configured(wire_pin(Pad::Mgpio8), wire_pin(Pad::Mgpio9)) };
    emit(&uart, b"WS53 I2C QEMU begin\n");
    let result = I2c::new_i2c0(
        unsafe { p.I2C0.assume_application_ownership() },
        pins0,
        config,
        delay,
    );
    if cfg!(feature = "clock-stall-smoke") {
        // A controller already disabled at handoff can be configured through
        // its live bus interface even though the peripheral source is stopped.
        // Enable must time out; cleanup observes it never became active.
        let mut driver = result.unwrap_or_else(|_| panic!("disabled handoff failed"));
        for _ in 0..2 {
            let error = driver
                .transaction(Address::seven(0x50).unwrap(), &mut [Operation::Write(&[0])])
                .unwrap_err();
            assert_eq!(error, Error::Failure(Failure::Timeout(Wait::Enable)));
            assert!(!driver.is_poisoned());
        }
        driver.recover().unwrap();
        assert!(driver.try_free().is_ok());
    } else if cfg!(any(
        feature = "arbitration-smoke",
        feature = "stuck-smoke",
        feature = "mixed-abort-smoke"
    )) {
        // Synthetic faults on I2C0. Check both raw words and standard kind after
        // cleanup; I2C1 below must remain usable.
        let mut driver = result.unwrap_or_else(|_| panic!("I2C0 init failed"));
        let (source, detail, kind) = if cfg!(feature = "arbitration-smoke") {
            (0, 0x1000, ErrorKind::ArbitrationLoss)
        } else if cfg!(feature = "stuck-smoke") {
            (2, 0, ErrorKind::Bus)
        } else {
            (2, 1, ErrorKind::Other)
        };
        for _ in 0..2 {
            let error = driver
                .transaction(Address::seven(0x50).unwrap(), &mut [Operation::Write(&[0])])
                .unwrap_err();
            assert_eq!(error, Error::Failure(Failure::Abort { source, detail }));
            assert_eq!(error.kind(), kind);
            assert!(!driver.is_poisoned());
        }
        emit(&uart, b"WS53 I2C clear fault\n");
        let mut budget = 50_000_000u32;
        loop {
            if let Ok(byte) = uart.read_byte() {
                assert_eq!(byte, b'R');
                break;
            }
            budget = budget.checked_sub(1).expect("fault control timeout");
        }
        driver
            .transaction(Address::seven(0x50).unwrap(), &mut [Operation::Write(&[0])])
            .unwrap();
        assert!(driver.try_free().is_ok());
    } else {
        let mut driver = result.unwrap_or_else(|_| panic!("I2C0 init failed"));
        exercise(&mut driver);
        let (token, pins) = driver
            .try_free()
            .unwrap_or_else(|_| panic!("I2C0 shutdown failed"));
        // Consume and re-use the exact same exclusive permissions.
        let mut driver = I2c::new_i2c0(token, pins, config, delay)
            .unwrap_or_else(|_| panic!("I2C0 reinit failed"));
        exercise(&mut driver);
        drop(driver);
    }
    // Instance1 keeps working even when instance0 has lost its source clock.
    let mut driver = I2c::new_i2c1(
        unsafe { p.I2C1.assume_application_ownership() },
        pins1,
        config,
        delay,
    )
    .unwrap_or_else(|_| panic!("I2C1 init failed"));
    exercise(&mut driver);
    let (_, pins) = driver
        .try_free()
        .unwrap_or_else(|_| panic!("I2C1 shutdown failed"));
    let (scl, sda) = pins.into_pads();
    let expected_pads = if cfg!(feature = "wire-pads") {
        (Pad::Mgpio8, Pad::Mgpio9)
    } else {
        (Pad::Mgpio5, Pad::Mgpio8)
    };
    assert_eq!((scl.pad(), sda.pad()), expected_pads);
    assert!(!interrupt::is_enabled(ExternalInterrupt::I2C_0));
    assert!(!interrupt::is_enabled(ExternalInterrupt::I2C_1));
    if cfg!(feature = "clock-stall-smoke") {
        emit(
            &uart,
            b"WS53 PASS I2C clock_stall enable_timeout=1 bounded=1 other_instance=1\n",
        );
    } else if cfg!(feature = "arbitration-smoke") {
        emit(
            &uart,
            b"WS53 PASS I2C arbitration_loss=1 raw=1 retry=1 other_instance=1\n",
        );
    } else if cfg!(feature = "stuck-smoke") {
        emit(
            &uart,
            b"WS53 PASS I2C stuck=1 raw=1 retry=1 other_instance=1\n",
        );
    } else if cfg!(feature = "mixed-abort-smoke") {
        emit(
            &uart,
            b"WS53 PASS I2C mixed_abort=1 raw=1 retry=1 other_instance=1\n",
        );
    } else {
        emit(&uart, b"WS53 PASS I2C instances=2 addr7=1 addr10=1 repeated_start=1 long_read=100 nack_recovery=1\n");
    }
    timer.borrow_mut().disable(TimerChannel::Channel1);
    emit(&uart, b"WS53 I2C QEMU done\n");
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
