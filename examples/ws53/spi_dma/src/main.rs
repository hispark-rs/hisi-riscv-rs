//! Model-only SPI duplex DMA fixture. Never load this image on physical boards.
#![no_std]
#![no_main]
use core::{cell::RefCell, num::NonZeroU16};
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    Peripherals,
    dma::{
        Buffer, Channel, Controller, Error,
        spi::{Instance, SpiDma},
    },
    peripherals::Uart2,
    pinctrl::PadControl,
    soc::ws53::pins::Pad,
    spi::Pins,
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::Uart,
};
use hisi_riscv_rt::entry;
#[cfg(feature = "width32")]
type Word = u32;
#[cfg(not(feature = "width32"))]
type Word = u8;
#[unsafe(no_mangle)]
static mut WS53_SPI_DMA_SOURCE: [Word; 35] = [0x5a; 35];
#[unsafe(no_mangle)]
static mut WS53_SPI_DMA_DESTINATION: [Word; 35] = [0x5a; 35];
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
fn pattern(port: u8, ch: u8, i: usize) -> Word {
    (0x12340007u32.wrapping_add(u32::from(port) * 53 + u32::from(ch) * 29 + (i as u32) * 0x10213))
        as Word
}
struct Case {
    port: u8,
    ch: u8,
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
    let Case { port, ch, scenario } = case;
    let length = if scenario == b'N' { 33 } else { 3 };
    let src = core::ptr::addr_of_mut!(WS53_SPI_DMA_SOURCE).cast::<Word>();
    let dst = core::ptr::addr_of_mut!(WS53_SPI_DMA_DESTINATION).cast::<Word>();
    // Explicitly simulated routes; these are not a recommendation for board wiring.
    let pins = unsafe {
        Pins::from_configured(
            PadControl::claim(Pad::Mgpio3).unwrap(),
            PadControl::claim(Pad::Mgpio4).unwrap(),
            PadControl::claim(Pad::Mgpio5).unwrap(),
        )
    };
    let (source, destination) = unsafe {
        for i in 0..length {
            src.add(i + 1).write(pattern(port, ch, i));
        }
        (
            Buffer::from_coherent_static(core::slice::from_raw_parts_mut(src.add(1), length))
                .ok()
                .unwrap(),
            Buffer::from_coherent_static(core::slice::from_raw_parts_mut(dst.add(1), length))
                .ok()
                .unwrap(),
        )
    };
    let spi = unsafe { SpiDma::<_, Word>::from_configured(token, pins) };
    let mut transfer = Some(spi.transfer(tx, rx, source, destination).ok().unwrap());
    let start = transfer.as_mut().unwrap().start();
    match scenario {
        b'C' => assert_eq!(start, Err(Error::SpiConfiguration)),
        b'B' => assert_eq!(start, Err(Error::SpiBusy)),
        b'L' | b'Z' => match start {
            Err(Error::SpiCollision { status, raw_interrupt }) => {
                assert_ne!(status & if port == 0 { 1 } else { 1 << 6 }, 0);
                let irq_bit = if port == 0 { 1 << 15 } else { 1 << 5 };
                assert_eq!(raw_interrupt & irq_bit != 0, scenario == b'L');
            }
            _ => panic!("expected collision status"),
        },
        _ => {
            start.unwrap();
            emit(console, b"WS53 SPIDMA armed\n");
            assert_eq!(receive(console), b'G');
            if matches!(scenario, b'D' | b'F' | b'I') {
                delay.delay_us(50);
                match scenario {
                    b'D' => drop(transfer.take().unwrap()),
                    b'F' => core::mem::forget(transfer.take().unwrap()),
                    _ => transfer.as_mut().unwrap().request_stop(),
                }
            } else {
                let result = transfer
                    .as_mut()
                    .unwrap()
                    .wait(NonZeroU16::new(1000).unwrap(), delay);
                match scenario {
                    b'N' => assert_eq!(result.unwrap().elements(), 33),
                    b'R' | b'W' => assert_eq!(result, Err(Error::Bus)),
                    _ => assert_eq!(result, Err(Error::Timeout)),
                }
            }
        }
    }
    if let Some(t) = transfer.as_mut() {
        assert_eq!(t.start(), Err(Error::NotReady));
    }
    emit(console, b"WS53 SPIDMA retained\n");
    // Host checks model transactions/serial completion/FIFO history before return.
    // No physical drain or cache proof can be inferred from this handshake.
    assert_eq!(receive(console), b'R');
    let copied = usize::from(receive(console));
    assert!(copied <= length);
    if let Some(t) = transfer {
        let (_spi, _tx, _rx, source, destination) =
            unsafe { t.release_after_external_quiescence() };
        for (i, &word) in source.into_words().iter().enumerate() {
            assert_eq!(word, pattern(port, ch, i));
        }
        for (i, &word) in destination.into_words().iter().enumerate() {
            assert_eq!(
                word,
                if i < copied {
                    pattern(port, ch, i) ^ (0xa5a5a5a5u32 as Word)
                } else {
                    0x5a
                }
            );
        }
    }
    emit(
        console,
        b"WS53 PASS SPIDMA retained=1 data=1\nWS53 SPIDMA done\n",
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
    emit(&console, b"WS53 SPIDMA ready\n");
    let port = receive(&mut console) - b'0';
    let ch = receive(&mut console) - b'0';
    let scenario = receive(&mut console);
    assert!(port < 2 && ch < 8);
    let tx = channels[ch as usize].take().unwrap();
    let rx = channels[(ch as usize + 1) % 8].take().unwrap();
    let case = Case { port, ch, scenario };
    if port == 0 {
        test(
            unsafe { p.SPI0.assume_application_ownership() },
            tx,
            rx,
            case,
            &mut console,
            &mut delay,
        );
    } else {
        test(
            unsafe { p.QSPI1.assume_application_ownership() },
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
