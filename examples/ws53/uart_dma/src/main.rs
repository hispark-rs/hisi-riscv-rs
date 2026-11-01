//! QEMU-only UART DMA ownership/serializer fixture. Never load on physical boards.
#![no_std]
#![no_main]
use core::cell::RefCell;
#[cfg(not(feature = "async-smoke"))]
use core::num::NonZeroU16;
#[cfg(feature = "async-smoke")]
mod asynch;
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    Peripherals,
    dma::{
        Buffer, Channel, Controller, Error,
        uart::{Instance, UartDma},
    },
    peripherals::Uart2,
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::Uart,
};
use hisi_riscv_rt::entry;
#[unsafe(no_mangle)]
static mut WS53_UART_DMA_BUFFER: [u8; 98] = [0x5a; 98];
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
fn emit(u: &Uart<'_, Uart2<'_>>, s: &[u8]) {
    for &b in s {
        let mut left = 1_000_000;
        while u.try_write_byte(b).is_err() {
            left -= 1;
            assert!(left != 0);
        }
    }
}
fn receive(u: &mut Uart<'_, Uart2<'_>>) -> u8 {
    for _ in 0..100_000_000 {
        if let Ok(b) = u.read_byte() {
            return b;
        }
    }
    panic!("handshake timeout");
}
fn pattern(port: u8, ch: u8, i: usize) -> u8 {
    (usize::from(port) * 53 + usize::from(ch) * 29 + i * 13 + 7) as u8
}
struct Case {
    port: u8,
    ch: u8,
    rx: bool,
    scenario: u8,
}
fn test<T: Instance + 'static>(
    token: T,
    channel: Channel<'static>,
    case: Case,
    console: &mut Uart<'_, Uart2<'_>>,
    delay: &mut impl DelayNs,
) {
    let Case {
        port,
        ch,
        rx,
        scenario,
    } = case;
    let length = if scenario == b'Q' { 96 } else { 17 };
    let pointer = core::ptr::addr_of_mut!(WS53_UART_DMA_BUFFER).cast::<u8>();
    // Model-only static/coherent/application-owned memory. Initialize before DMA.
    let buffer = unsafe {
        if !rx {
            for i in 0..length {
                pointer.add(i + 1).write(pattern(port, ch, i));
            }
        }
        Buffer::from_coherent_static(core::slice::from_raw_parts_mut(pointer.add(1), length))
            .ok()
            .unwrap()
    };
    let uart = unsafe { UartDma::from_configured(token) };
    let mut t = Some(if rx {
        uart.receive(channel, buffer)
    } else {
        uart.transmit(channel, buffer)
    });
    #[cfg(feature = "async-smoke")]
    asynch::exercise(&mut t, rx, scenario, length, console, delay);
    #[cfg(not(feature = "async-smoke"))]
    {
        let start = t.as_mut().unwrap().start();
        if matches!(scenario, b'C' | b'B' | b'L') {
            match scenario {
                b'C' => assert_eq!(start, Err(Error::UartCapability)),
                b'B' => assert_eq!(start, Err(Error::UartBusy)),
                _ => assert!(matches!(start,Err(Error::UartLine(e)) if e.parity())),
            }
        } else {
            start.unwrap();
            emit(console, b"WS53 UARTDMA armed\n");
            assert_eq!(receive(console), b'G');
            if matches!(scenario, b'D' | b'F' | b'I') {
                delay.delay_us(20);
                match scenario {
                    b'D' => drop(t.take().unwrap()),
                    b'F' => core::mem::forget(t.take().unwrap()),
                    _ => t.as_mut().unwrap().request_stop(),
                }
            } else {
                let result = t
                    .as_mut()
                    .unwrap()
                    .wait(NonZeroU16::new(1000).unwrap(), delay);
                match scenario {
                    b'N' | b'V' => assert_eq!(result.unwrap().elements(), length as u16),
                    b'R' | b'W' => assert_eq!(result, Err(Error::Bus)),
                    _ => assert_eq!(result, Err(Error::Timeout)),
                }
                if matches!(scenario, b'N' | b'V') {
                    if rx {
                        assert_eq!(
                            t.as_mut()
                                .unwrap()
                                .wait_transmitter_idle(NonZeroU16::new(1).unwrap(), delay),
                            Err(Error::NotReady)
                        );
                    } else {
                        assert_eq!(
                            t.as_mut()
                                .unwrap()
                                .wait_transmitter_idle(NonZeroU16::new(1).unwrap(), delay),
                            Err(Error::TransmitTimeout)
                        );
                        emit(console, b"WS53 UARTDMA accepted\n");
                        assert_eq!(receive(console), b'H');
                        let idle = t
                            .as_mut()
                            .unwrap()
                            .wait_transmitter_idle(NonZeroU16::new(4000).unwrap(), delay);
                        assert_eq!(
                            idle,
                            if scenario == b'V' {
                                Err(Error::TransmitTimeout)
                            } else {
                                Ok(())
                            }
                        );
                    }
                }
            }
        }
    }
    if let Some(t) = t.as_mut() {
        assert_eq!(t.start(), Err(Error::NotReady));
    }
    emit(console, b"WS53 UARTDMA retained\n");
    // Host proves no pending model beat or peripheral transmission remains;
    // physical FIFO/ACK/cache/drain cannot be inferred from this protocol.
    assert_eq!(receive(console), b'R');
    if let Some(t) = t {
        let (_uart, _channel, buffer) = unsafe { t.release_after_external_quiescence() };
        let bytes = buffer.into_words();
        let copied = if !rx || scenario == b'N' {
            length
        } else if matches!(scenario, b'T' | b'I') {
            1
        } else {
            0
        };
        for (i, &byte) in bytes.iter().enumerate() {
            assert_eq!(
                byte,
                if i < copied {
                    pattern(port, ch, i)
                } else {
                    0x5a
                }
            );
        }
    }
    #[cfg(not(feature = "async-smoke"))]
    emit(console, b"WS53 PASS UARTDMA retained=1 data=1\n");
    #[cfg(feature = "async-smoke")]
    emit(console, b"WS53 PASS UARTDMA ASYNC retained=1 data=1\n");
    emit(console, b"WS53 UARTDMA done\n");
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
    emit(&console, b"WS53 UARTDMA ready\n");
    let port = receive(&mut console) - b'0';
    let ch = receive(&mut console) - b'0';
    let rx = receive(&mut console) == b'r';
    let scenario = receive(&mut console);
    assert!(port < 2 && ch < 8);
    let channel = channels[ch as usize].take().unwrap();
    let case = Case {
        port,
        ch,
        rx,
        scenario,
    };
    if port == 0 {
        test(
            unsafe { p.UART0.assume_application_ownership() },
            channel,
            case,
            &mut console,
            &mut delay,
        );
    } else {
        test(
            unsafe { p.UART1.assume_application_ownership() },
            channel,
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
fn panic(info: &core::panic::PanicInfo) -> ! {
    // Diagnostic only: execution is terminal, the console is never borrowed again.
    let regs = unsafe { &*Uart2::ptr() };
    let line = info.location().map_or(0, |loc| loc.line());
    for b in b"WS53 UARTDMA PANIC "
        .iter()
        .copied()
        .chain(
            (0..8)
                .rev()
                .map(|i| b"0123456789abcdef"[((line >> (i * 4)) & 15) as usize]),
        )
        .chain(*b"\n")
    {
        unsafe {
            regs.tx().write_with_zero(|w| w.data().bits(b));
        }
    }
    loop {
        core::hint::spin_loop();
    }
}
