//! QEMU-only eight-channel MDMA ownership fixture; never run on physical boards.
#![no_std]
#![no_main]
use core::{cell::RefCell, num::NonZeroU16};
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    Peripherals,
    dma::{Buffer, Controller, Error, Transfer},
    peripherals::Uart2,
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::Uart,
};
use hisi_riscv_rt::entry;
#[cfg(all(feature = "width8", feature = "width16"))]
compile_error!("select only one DMA element width");
#[cfg(feature = "width8")]
type Word = u8;
#[cfg(all(feature = "width16", not(feature = "width8")))]
type Word = u16;
#[cfg(not(any(feature = "width8", feature = "width16")))]
type Word = u32;
#[unsafe(no_mangle)]
static mut WS53_DMA_SOURCE: [[Word; 19]; 8] = [[0x5a; 19]; 8];
#[unsafe(no_mangle)]
static mut WS53_DMA_DESTINATION: [[Word; 19]; 8] = [[0x5a; 19]; 8];
fn value(channel: usize, index: usize) -> Word {
    (channel * 29 + index * 13 + 7) as Word
}
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
    panic!("host handshake timeout");
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    // Explicit QEMU profile only: no competing core, cache aliases or old DMA.
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
    let channels = Controller::new(unsafe { p.MDMA.assume_application_ownership() })
        .split()
        .ok()
        .unwrap();
    let channels = [
        channels.channel0,
        channels.channel1,
        channels.channel2,
        channels.channel3,
        channels.channel4,
        channels.channel5,
        channels.channel6,
        channels.channel7,
    ];
    let mut transfers: [Option<Transfer<'static, Word>>; 8] = [const { None }; 8];
    for (index, channel) in channels.into_iter().enumerate() {
        assert_eq!(channel.id() as usize, index);
        unsafe {
            let source = core::ptr::addr_of_mut!(WS53_DMA_SOURCE)
                .cast::<Word>()
                .add(index * 19 + 1);
            let destination = core::ptr::addr_of_mut!(WS53_DMA_DESTINATION)
                .cast::<Word>()
                .add(index * 19 + 1);
            // CPU writes occur before giving away either memory owner.
            for n in 0..17 {
                source.add(n).write(value(index, n));
            }
            let source = Buffer::from_coherent_static(core::slice::from_raw_parts_mut(source, 17))
                .ok()
                .unwrap();
            let destination =
                Buffer::from_coherent_static(core::slice::from_raw_parts_mut(destination, 17))
                    .ok()
                    .unwrap();
            transfers[index] = Some(Transfer::new(channel, source, destination).ok().unwrap());
        }
    }
    emit(&uart, b"WS53 MDMA begin\n");
    let scenario = receive(&mut uart);
    for t in &mut transfers {
        t.as_mut().unwrap().start().unwrap();
    }
    if matches!(scenario, b'D' | b'F' | b'I') {
        delay.delay_us(20);
        match scenario {
            b'D' => drop(transfers[0].take().unwrap()),
            b'F' => core::mem::forget(transfers[0].take().unwrap()),
            _ => transfers[0].as_mut().unwrap().request_stop(),
        }
    }
    for (index, t) in transfers.iter_mut().enumerate() {
        if index == 0 && matches!(scenario, b'D' | b'F' | b'I') {
            continue;
        }
        let t = t.as_mut().unwrap();
        let result = t.wait(NonZeroU16::new(1000).unwrap(), &mut delay);
        if index != 0 || scenario == b'N' {
            assert_eq!(result.unwrap().elements(), 17);
        } else {
            assert_eq!(
                result,
                Err(if matches!(scenario, b'R' | b'W') {
                    Error::Bus
                } else {
                    Error::Timeout
                })
            );
        }
        assert_eq!(t.start(), Err(Error::NotReady));
    }
    emit(&uart, b"WS53 MDMA retained\n");
    // Host checks all channels, addresses, RAM and pending writes independently;
    // then it grants the model-only drain proof. Not a physical stop protocol.
    assert_eq!(receive(&mut uart), b'R');
    for (index, t) in transfers.into_iter().enumerate() {
        let Some(t) = t else {
            continue;
        };
        let (_channel, source, destination) = unsafe { t.release_after_external_quiescence() };
        let source = source.into_words();
        let destination = destination.into_words();
        let copied = if index != 0 || scenario == b'N' {
            17
        } else if matches!(scenario, b'T' | b'I') {
            1
        } else {
            0
        };
        for n in 0..17 {
            assert_eq!(source[n], value(index, n));
            assert_eq!(
                destination[n],
                if n < copied { value(index, n) } else { 0x5a }
            );
        }
    }
    emit(&uart, b"WS53 PASS MDMA channels=8 retained=1 data=1\n");
    emit(&uart, b"WS53 MDMA done\n");
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
