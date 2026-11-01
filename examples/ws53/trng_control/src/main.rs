//! QEMU-only TRNG test source. Never load on a physical board or use as entropy.
#![no_std]
#![no_main]
use core::{cell::RefCell, num::NonZeroU16};
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    Peripherals,
    peripherals::Uart2,
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    trng::{Error, TrngDriver},
    uart::Uart,
};
use hisi_riscv_rt::entry;
#[unsafe(no_mangle)]
static mut WS53_TRNG_OUTPUT: [u8; 20] = [0xa5; 20];
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

fn word(u: &mut Uart<'_, Uart2<'_>>) -> u32 {
    u32::from_le_bytes([receive(u), receive(u), receive(u), receive(u)])
}
fn next(state: &mut u32) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state
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
    emit(&console, b"WS53 TRNG ready\n");
    let scenario = receive(&mut console);
    let len = receive(&mut console) as usize;
    let mut seed = word(&mut console);
    assert!(len <= 17);
    // Host has configured and settled the explicit model; no real service handoff.
    let mut source = unsafe { TrngDriver::from_configured(p.TRNG.assume_application_ownership()) };
    emit(&console, b"WS53 TRNG attached\n");
    assert_eq!(receive(&mut console), b'G');
    let data = unsafe { &mut *core::ptr::addr_of_mut!(WS53_TRNG_OUTPUT) };
    let result = source.fill_bytes(
        &mut data[1..1 + len],
        NonZeroU16::new(if scenario == b'P' { 1 } else { 1000 }).unwrap(),
        &mut delay,
    );
    let written = match scenario {
        b'N' | b'X' => {
            assert_eq!(result, Ok(()));
            len
        }
        b'P' => {
            let e = result.unwrap_err();
            assert_eq!((e.written(), e.error()), (4, Error::Timeout));
            4
        }
        b'R' => {
            let e = result.unwrap_err();
            assert_eq!((e.written(), e.error()), (0, Error::RingDisabled));
            0
        }
        _ => {
            let e = result.unwrap_err();
            assert_eq!((e.written(), e.error()), (0, Error::Timeout));
            0
        }
    };
    let mut expected = [0xa5; 20];
    for chunk in expected[1..1 + written].chunks_mut(4) {
        let candidate = next(&mut seed);
        let _comparison = next(&mut seed);
        chunk.copy_from_slice(&candidate.to_le_bytes()[..chunk.len()]);
    }
    assert_eq!(data, &expected);
    emit(&console, b"WS53 TRNG checked\n");
    // Host independently restores source and sends the next two expected candidates.
    let first = word(&mut console);
    let second = word(&mut console);
    assert_eq!(receive(&mut console), b'H');
    let token = source.release_without_stopping();
    {
        let mut source = unsafe { TrngDriver::from_configured(token) };
        assert_eq!(source.try_read_word(), Ok(Some(first)));
        assert_eq!(
            source.read_word(NonZeroU16::new(1000).unwrap(), &mut delay),
            Ok(second)
        );
        assert_eq!(
            source.fill_bytes(&mut [], NonZeroU16::new(1).unwrap(), &mut delay),
            Ok(())
        );
    }
    emit(
        &console,
        b"WS53 PASS TRNG data=1 prefix=1 retry=1\nWS53 TRNG done\n",
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
