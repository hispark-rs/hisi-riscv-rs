//! QEMU-only I2S DMA fixture. Never load on a physical board.
#![no_std]
#![no_main]
use core::{cell::RefCell, num::NonZeroU16};
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    Peripherals,
    dma::{
        Buffer, Controller, Error,
        i2s::{Burst, I2sDma, Samples},
    },
    i2s::{BclkDivider, ClockEdge, ClockMode, DataWidth, Pins},
    peripherals::Uart2,
    pinctrl::PadControl,
    soc::ws53::pins::Pad,
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::Uart,
};
use hisi_riscv_rt::entry;
#[unsafe(no_mangle)]
static mut WS53_I2S_DMA_SOURCE: [u32; 70] = [0x5a5a5a5a; 70];
#[unsafe(no_mangle)]
static mut WS53_I2S_DMA_DESTINATION: [u32; 70] = [0x5a5a5a5a; 70];
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
    emit(&console, b"WS53 I2SDMA ready\n");
    let ch = receive(&mut console) - b'0';
    let width_code = receive(&mut console) - b'1';
    let width = [
        DataWidth::Bits16,
        DataWidth::Bits18,
        DataWidth::Bits20,
        DataWidth::Bits24,
        DataWidth::Bits32,
    ][width_code as usize];
    let burst = [Burst::One, Burst::Four, Burst::Eight, Burst::Sixteen]
        [(receive(&mut console) - b'0') as usize];
    let kind = receive(&mut console);
    let scenario = receive(&mut console);
    let slave = receive(&mut console) == b's';
    let clock = if slave {
        ClockMode::Slave
    } else {
        ClockMode::Master(BclkDivider::new(100).unwrap())
    };
    let normal = matches!(scenario, b'N' | b'2' | b'6');
    let reads = if kind == b'w' {
        0
    } else if scenario == b'2' {
        2
    } else if scenario == b'6' {
        6
    } else if kind == b'c' {
        48
    } else {
        32
    };
    let writes = if kind == b'r' {
        0
    } else if kind == b'c' {
        reads + 16
    } else {
        32
    };
    let mask = u32::MAX >> (32 - width.bits());
    let sample = |n: usize| ((n as u32 + 1).wrapping_mul(0x9137137) + u32::from(ch)) & mask;
    let src = core::ptr::addr_of_mut!(WS53_I2S_DMA_SOURCE).cast::<u32>();
    let dst = core::ptr::addr_of_mut!(WS53_I2S_DMA_DESTINATION).cast::<u32>();
    let tx = if writes == 0 {
        None
    } else {
        let data = unsafe { core::slice::from_raw_parts_mut(src.add(1), writes) };
        for (n, x) in data.iter_mut().enumerate() {
            *x = sample(n);
        }
        let b = unsafe { Buffer::from_coherent_static(data) }.ok().unwrap();
        Some((
            channels[ch as usize].take().unwrap(),
            Samples::new(b, width).ok().unwrap(),
        ))
    };
    let rx = if reads == 0 {
        None
    } else {
        Some((
            channels[(ch as usize + 1) % 8].take().unwrap(),
            unsafe {
                Buffer::from_coherent_static(core::slice::from_raw_parts_mut(dst.add(1), reads))
            }
            .ok()
            .unwrap(),
        ))
    };
    let pins = unsafe {
        Pins::from_configured(
            PadControl::claim(Pad::Mgpio17).unwrap(),
            PadControl::claim(Pad::Mgpio20).unwrap(),
            PadControl::claim(Pad::Mgpio19).unwrap(),
            PadControl::claim(Pad::Mgpio18).unwrap(),
            clock,
        )
    };
    let owner = unsafe {
        I2sDma::from_configured(
            p.I2S.assume_application_ownership(),
            pins,
            width,
            clock,
            ClockEdge::Rising,
            burst,
        )
    };
    let mut transfer = Some(owner.transfer(tx, rx).ok().unwrap());
    let start = transfer.as_mut().unwrap().start();
    match scenario {
        b'C' => assert_eq!(start, Err(Error::I2sConfiguration)),
        b'B' => assert_eq!(start, Err(Error::I2sBusy)),
        _ => {
            start.unwrap();
            emit(&console, b"WS53 I2SDMA armed\n");
            assert_eq!(receive(&mut console), b'G');
            if matches!(scenario, b'D' | b'F' | b'I') {
                delay.delay_us(1000);
                match scenario {
                    b'D' => drop(transfer.take().unwrap()),
                    b'F' => core::mem::forget(transfer.take().unwrap()),
                    _ => transfer.as_mut().unwrap().request_stop(),
                }
            } else {
                let result = transfer.as_mut().unwrap().wait_dma(
                    NonZeroU16::new(if normal { 20000 } else { 1000 }).unwrap(),
                    &mut delay,
                );
                if normal {
                    let progress = result.unwrap();
                    assert_eq!(
                        (progress.tx_samples(), progress.rx_samples()),
                        (writes as u16, reads as u16)
                    );
                } else if matches!(scenario, b'R' | b'W') {
                    assert_eq!(result, Err(Error::Bus));
                } else {
                    assert!(matches!(
                        result,
                        Err(Error::Timeout) | Err(Error::I2sHardware { .. })
                    ));
                }
            }
        }
    }
    if let Some(t) = &mut transfer {
        assert_eq!(t.start(), Err(Error::NotReady));
    }
    emit(&console, b"WS53 I2SDMA retained\n");
    assert_eq!(receive(&mut console), b'R');
    let copied = receive(&mut console) as usize;
    assert!(copied <= reads);
    if let Some(t) = transfer {
        let (_owner, tx, rx) = unsafe { t.release_after_external_quiescence() };
        if let Some((_, s)) = tx {
            for (n, &v) in s.into_buffer().into_words().iter().enumerate() {
                assert_eq!(v, sample(n));
            }
        }
        if let Some((_, b)) = rx {
            for (n, &v) in b.into_words().iter().enumerate() {
                let expected = if n >= copied {
                    0x5a5a5a5a
                } else if kind == b'c' {
                    sample(n) ^ (if n % 2 == 0 { 0xa5a5 } else { 0x5a5a })
                } else {
                    let frame = n as u32 / 2 + 1;
                    (if n % 2 == 0 { frame } else { !frame }) & mask
                };
                assert_eq!(v, expected);
            }
        }
    }
    emit(
        &console,
        b"WS53 PASS I2SDMA retained=1 data=1\nWS53 I2SDMA done\n",
    );
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
