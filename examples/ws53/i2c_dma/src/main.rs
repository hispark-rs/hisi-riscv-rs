//! Model-only I2C command DMA fixture. Never load this image on physical boards.
#![no_std]
#![no_main]
use core::{cell::RefCell, num::NonZeroU16};
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    Peripherals,
    dma::{
        Buffer, Channel, Controller, Error,
        i2c::{Burst, Commands, I2cDma, Instance},
    },
    i2c::{Address, Pins},
    peripherals::Uart2,
    pinctrl::PadControl,
    soc::ws53::pins::Pad,
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::Uart,
};
use hisi_riscv_rt::entry;
#[unsafe(no_mangle)]
static mut WS53_I2C_DMA_SOURCE: [u16; 35] = [0x5a; 35];
#[unsafe(no_mangle)]
static mut WS53_I2C_DMA_DESTINATION: [u8; 35] = [0x5a; 35];
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
            if start.wrapping_sub(
                self.0
                    .borrow_mut()
                    .current_value(TimerChannel::Channel1)
                    .unwrap(),
            ) >= ticks
            {
                return;
            }
        }
        panic!("timer stopped");
    }
}
fn emit(u: &Uart<'_, Uart2<'_>>, s: &[u8]) {
    for &b in s {
        let mut n = 1_000_000;
        while u.try_write_byte(b).is_err() {
            n -= 1;
            assert!(n != 0);
        }
    }
}
fn receive(u: &mut Uart<'_, Uart2<'_>>) -> u8 {
    for _ in 0..100_000_000 {
        if let Ok(b) = u.read_byte() {
            return b;
        }
    }
    panic!("host timeout");
}
struct Case {
    port: u8,
    ch: u8,
    kind: u8,
    burst: Burst,
    scenario: u8,
}
fn test<T: Instance + 'static>(
    token: T,
    tx: Channel<'static>,
    rx: Channel<'static>,
    case: Case,
    console: &mut Uart<'_, Uart2<'_>>,
    delay: &mut impl DelayNs,
) {
    let Case {
        port,
        ch,
        kind,
        burst,
        scenario,
    } = case;
    let ten = kind.is_ascii_uppercase();
    let kind = kind.to_ascii_lowercase();
    let normal = matches!(scenario, b'N' | b'1' | b'3' | b'4');
    let length = if scenario == b'N' {
        33
    } else if normal {
        usize::from(scenario - b'0')
    } else {
        5
    };
    let reads = if kind == b'w' {
        0
    } else if kind == b'c' {
        length - 1
    } else {
        length
    };
    let src = core::ptr::addr_of_mut!(WS53_I2C_DMA_SOURCE).cast::<u16>();
    let dst = core::ptr::addr_of_mut!(WS53_I2C_DMA_DESTINATION).cast::<u8>();
    // Model-only routes; not recommended physical board wiring.
    let pins = unsafe {
        Pins::from_configured(
            PadControl::claim(Pad::Mgpio3).unwrap(),
            PadControl::claim(Pad::Mgpio4).unwrap(),
        )
    };
    let source = unsafe {
        Buffer::from_coherent_static(core::slice::from_raw_parts_mut(src.add(1), length))
            .ok()
            .unwrap()
    };
    let mut bytes = [0u8; 33];
    bytes[0] = 0x20;
    for (n, b) in bytes.iter_mut().enumerate().take(length).skip(1) {
        *b = (n as u8)
            .wrapping_mul(7)
            .wrapping_add(ch)
            .wrapping_add(port * 29);
    }
    let commands = match kind {
        b'w' => Commands::write(source, &bytes[..length]).ok().unwrap(),
        b'c' => Commands::write_read(source, &[0x20], NonZeroU16::new(reads as u16).unwrap())
            .ok()
            .unwrap(),
        _ => Commands::read(source),
    };
    let receive_buffer = if reads == 0 {
        None
    } else {
        Some((rx, unsafe {
            Buffer::from_coherent_static(core::slice::from_raw_parts_mut(dst.add(1), reads))
                .ok()
                .unwrap()
        }))
    };
    let i2c = unsafe {
        I2cDma::from_configured(
            token,
            pins,
            if ten {
                Address::ten(0x250).unwrap()
            } else {
                Address::seven(0x50).unwrap()
            },
            burst,
        )
    };
    let mut transfer = Some(i2c.transfer(tx, commands, receive_buffer).ok().unwrap());
    let start = transfer.as_mut().unwrap().start();
    match scenario {
        b'C' => assert_eq!(start, Err(Error::I2cConfiguration)),
        b'B' => assert_eq!(start, Err(Error::I2cBusy)),
        b'L' => assert!(matches!(start, Err(Error::I2cHardware { .. }))),
        _ => {
            start.unwrap();
            emit(console, b"WS53 I2CDMA armed\n");
            assert_eq!(receive(console), b'G');
            if matches!(scenario, b'D' | b'F' | b'I') {
                delay.delay_us(1000);
                match scenario {
                    b'D' => drop(transfer.take().unwrap()),
                    b'F' => core::mem::forget(transfer.take().unwrap()),
                    _ => transfer.as_mut().unwrap().request_stop(),
                }
            } else {
                let result = transfer.as_mut().unwrap().wait(
                    NonZeroU16::new(if normal { 20000 } else { 3000 }).unwrap(),
                    delay,
                );
                match scenario {
                    b'N' | b'1' | b'3' | b'4' => {
                        assert_eq!(result.unwrap().elements(), length as u16)
                    }
                    b'R' | b'W' => assert_eq!(result, Err(Error::Bus)),
                    b'A' => assert!(matches!(result, Err(Error::I2cHardware { .. }))),
                    _ => assert_eq!(result, Err(Error::Timeout)),
                }
            }
        }
    }
    if let Some(t) = transfer.as_mut() {
        assert_eq!(t.start(), Err(Error::NotReady));
    }
    emit(console, b"WS53 I2CDMA retained\n");
    // Host independently stops/drains model transactions and resets FIFO/bus.
    assert_eq!(receive(console), b'R');
    let copied = usize::from(receive(console));
    assert!(copied <= reads);
    if let Some(t) = transfer {
        let (_owner, _tx, commands, receive_buffer) =
            unsafe { t.release_after_external_quiescence() };
        let words = commands.into_buffer().into_words();
        for (n, &w) in words.iter().enumerate() {
            let mut expected = if kind == b'w' {
                u16::from(bytes[n])
            } else if kind == b'c' && n == 0 {
                0x20
            } else {
                0x100
            };
            if kind == b'c' && n == 1 {
                expected |= 0x400;
            }
            if n + 1 == length {
                expected |= 0x200;
            }
            assert_eq!(w, expected);
        }
        if let Some((_rx, buf)) = receive_buffer {
            for (n, &b) in buf.into_words().iter().enumerate() {
                assert_eq!(
                    b,
                    if n < copied {
                        ((if kind == b'c' { 0x20 } else { 0 }) + n as u8) ^ 0xa5
                    } else {
                        0x5a
                    }
                );
            }
        }
    }
    emit(
        console,
        b"WS53 PASS I2CDMA retained=1 data=1\nWS53 I2CDMA done\n",
    );
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let mut console = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
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
    let c = Controller::new(unsafe { p.MDMA.assume_application_ownership() })
        .split()
        .ok()
        .unwrap();
    let mut channels = [
        Some(c.channel0),
        Some(c.channel1),
        Some(c.channel2),
        Some(c.channel3),
        Some(c.channel4),
        Some(c.channel5),
        Some(c.channel6),
        Some(c.channel7),
    ];
    emit(&console, b"WS53 I2CDMA ready\n");
    let port = receive(&mut console) - b'0';
    let ch = receive(&mut console) - b'0';
    let kind = receive(&mut console);
    let burst = if receive(&mut console) == b'4' {
        Burst::Four
    } else {
        Burst::One
    };
    let scenario = receive(&mut console);
    assert!(port < 2 && ch < 8);
    let tx = channels[ch as usize].take().unwrap();
    let rx = channels[(ch as usize + 1) % 8].take().unwrap();
    let case = Case {
        port,
        ch,
        kind,
        burst,
        scenario,
    };
    if port == 0 {
        test(
            unsafe { p.I2C0.assume_application_ownership() },
            tx,
            rx,
            case,
            &mut console,
            &mut delay,
        );
    } else {
        test(
            unsafe { p.I2C1.assume_application_ownership() },
            tx,
            rx,
            case,
            &mut console,
            &mut delay,
        );
    }
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
