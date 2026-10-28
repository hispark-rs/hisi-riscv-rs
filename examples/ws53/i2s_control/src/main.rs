//! QEMU-only I2S fixture. Synthetic pad handoff and external quiescence proof
//! belong to the explicit QEMU profile, never to a physical board or audio codec.
#![no_std]
#![no_main]
use core::{cell::RefCell, num::NonZeroU16};
use embedded_hal::delay::DelayNs;
use hisi_hal::i2s::{
    BclkDivider, ClockEdge, ClockMode, Config, DataWidth, Error, Frame, I2sStream, Pins,
    StreamMode, TransferError, Wait,
};
use hisi_hal::peripherals::Uart2;
use hisi_hal::pinctrl::PadControl;
use hisi_hal::soc::ws53::pins::Pad;
use hisi_hal::timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks};
use hisi_hal::uart::Uart;
use hisi_hal::{Peripherals, interrupt};
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
fn frame(n: u32) -> Frame {
    Frame {
        left: n,
        right: n ^ 0x1234,
    }
}
fn exercise<D: DelayNs>(stream: &mut I2sStream<'_, D>, width: DataWidth) {
    let mut initial = [frame(0); 8];
    for (n, f) in initial.iter_mut().enumerate() {
        *f = frame(n as u32);
    }
    stream.start(StreamMode::Duplex, &initial).unwrap();
    // Leave seven future frames queued while reading each data-dependent reply.
    // This explicitly services both directions instead of overflowing RX during TX.
    for n in 0..100 {
        let mut output = [frame(0)];
        stream.read_frames(&mut output).unwrap();
        assert_eq!(output[0].left, n ^ 0xa5a5);
        assert_eq!(output[0].right, n ^ 0x1234 ^ 0x5a5a);
        stream.queue_frames(&[frame(n + 8)]).unwrap();
    }
    stream.discard_and_recover().unwrap(); // discards pending wire/FIFO data
    assert!(!stream.is_poisoned());
    stream.start(StreamMode::Receive, &[]).unwrap();
    let mut short = [frame(0)];
    stream.read_frames(&mut short).unwrap();
    let mask = if width == DataWidth::Bits32 {
        u32::MAX
    } else {
        (1 << width.bits()) - 1
    };
    assert_eq!(short[0].left ^ short[0].right, mask);
    stream.discard_and_recover().unwrap();
    stream.start(StreamMode::Transmit, &initial).unwrap();
    stream.wait_tx_fifo_empty().unwrap(); // no claim that the last frame completed
    stream.discard_and_recover().unwrap();
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
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
    let delay = TimerDelay(&timer);
    let mut token = unsafe { p.I2S.assume_application_ownership() };
    // Distinct synthetic pads, no physical mux/power configuration implied.
    let mut pads = unsafe {
        (
            PadControl::claim(Pad::Mgpio17).unwrap(),
            PadControl::claim(Pad::Mgpio20).unwrap(),
            PadControl::claim(Pad::Mgpio19).unwrap(),
            PadControl::claim(Pad::Mgpio18).unwrap(),
        )
    };
    emit(&uart, b"WS53 I2S QEMU begin\n");
    for width in [
        DataWidth::Bits16,
        DataWidth::Bits18,
        DataWidth::Bits20,
        DataWidth::Bits24,
        DataWidth::Bits32,
    ] {
        for clock in [
            ClockMode::Master(BclkDivider::new(127).unwrap()),
            ClockMode::Slave,
        ] {
            for edge in [ClockEdge::Rising, ClockEdge::Falling] {
                let pins = unsafe { Pins::from_configured(pads.0, pads.1, pads.2, pads.3, clock) };
                let config =
                    Config::new(width, clock, NonZeroU16::new(2048).unwrap()).with_edge(edge);
                let mut stream = I2sStream::new(token, pins, config, delay)
                    .unwrap_or_else(|_| panic!("I2S init failed"));
                if cfg!(feature = "clock-stall-smoke") {
                    stream.start(StreamMode::Duplex, &[frame(1); 8]).unwrap();
                    assert_eq!(
                        stream.queue_frames(&[frame(2)]),
                        Err(TransferError {
                            cause: Error::Timeout(Wait::TxSpace),
                            frames: 0
                        })
                    );
                    assert!(stream.is_poisoned());
                    assert_eq!(stream.try_queue(frame(2)), Err(Error::Poisoned));
                    // Readback/config recovery cannot prove the source clock runs.
                    // A second RX operation must still time out with clock stopped.
                    stream.discard_and_recover().unwrap();
                    stream.start(StreamMode::Receive, &[]).unwrap();
                    assert_eq!(
                        stream.read_frames(&mut [frame(0)]),
                        Err(TransferError {
                            cause: Error::Timeout(Wait::RxData),
                            frames: 0
                        })
                    );
                    stream.discard_and_recover().unwrap();
                } else {
                    exercise(&mut stream, width);
                }
                // QEMU CT_CLR reset cancels serializer, all FIFOs and requests.
                // This is an external MODEL proof, not a portable hardware proof.
                let pins;
                (token, pins) = unsafe { stream.release_after_external_quiescence() };
                pads = pins.into_pads();
            }
        }
    }
    assert!(!interrupt::is_enabled(ExternalInterrupt::I2S));
    if cfg!(feature = "clock-stall-smoke") {
        emit(
            &uart,
            b"WS53 PASS I2S clock_stall tx_rx_timeout=1 poisoned=1 bounded_recovery=1 cases=20\n",
        );
    } else {
        emit(
            &uart,
            b"WS53 PASS I2S widths=5 roles=2 edges=2 frames=100 short_rx=1 fifo_empty=1\n",
        );
    }
    timer.borrow_mut().disable(TimerChannel::Channel1);
    emit(&uart, b"WS53 I2S QEMU done\n");
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
