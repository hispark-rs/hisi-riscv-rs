//! Bounded TRNG observation; both vendor CPU clients are stopped.
//! Optional SDK ring startup. No clock/FRO writes, IPC handoff or entropy claim.
use core::num::NonZeroU16;
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    peripherals::{Trng, Uart2},
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    trng::TrngDriver,
    uart::Uart,
};

#[cfg(feature = "trng-ring-start-smoke")]
#[unsafe(no_mangle)]
pub static mut WS53_TRNG_STARTUP: [u32; 8] = [0; 8];
#[cfg(feature = "trng-ring-start-smoke")]
fn startup_word(index: usize, value: u32) {
    unsafe {
        core::ptr::addr_of_mut!(WS53_TRNG_STARTUP)
            .cast::<u32>()
            .add(index)
            .write_volatile(value)
    };
}

struct Delay<'a, 'd>(&'a mut TimerDriver<'d>);
impl DelayNs for Delay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        let start = self.0.current_value(TimerChannel::Channel1).unwrap();
        for _ in 0..10_000_000 {
            if start.wrapping_sub(self.0.current_value(TimerChannel::Channel1).unwrap()) >= ticks {
                return;
            }
        }
        panic!("independent TIMER stopped");
    }
}
fn hex(uart: &Uart<'_, Uart2<'_>>, value: u32) {
    for shift in (0..8).rev() {
        super::emit(
            uart,
            &[b"0123456789abcdef"[((value >> (shift * 4)) & 15) as usize]],
        );
    }
}
pub fn check(token: Trng<'_>, timer: &mut TimerDriver<'_>, uart: &mut Uart<'_, Uart2<'_>>) {
    super::emit(uart, b"WS53 TRNG HANDOFF\r\n");
    let mut received = false;
    for _ in 0..100_000_000 {
        if let Ok(byte) = uart.read_byte() {
            assert_eq!(byte, b'g');
            received = true;
            break;
        }
    }
    assert!(received, "host timeout");
    let ring = unsafe { &*ws53_pac::MCrg::PTR }.trng_ring_control();
    #[cfg(not(feature = "trng-ring-start-smoke"))]
    assert_eq!(ring.read().bits() & 1, 1);
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    let mut delay = Delay(timer);
    #[cfg(feature = "trng-ring-start-smoke")]
    let token = {
        startup_word(1, u32::from(ring.read().bits()));
        assert_eq!(ring.read().bits(), 0);
        let start = delay.0.current_value(TimerChannel::Channel1).unwrap();
        // The controlled host has excluded all CPU clients and established
        // accessible upstream power/clocks/source. It handed over ring=0.
        let driver = unsafe { TrngDriver::start_sdk_ring(token, &mut delay) }.unwrap();
        startup_word(
            3,
            start.wrapping_sub(delay.0.current_value(TimerChannel::Channel1).unwrap()),
        );
        startup_word(2, u32::from(ring.read().bits()));
        driver.release_without_stopping()
    };
    let inherited = ring.read().bits();
    assert_eq!(inherited & 1, 1);
    delay.delay_us(200);
    let regs = unsafe { &*ws53_pac::Trng::PTR };
    // First observe the SDK's double-ready/two-word sequence directly. This is
    // a diagnostic, not a universal FIFO-depth or entropy qualification.
    for _ in 0..16 {
        let mut pair = None;
        for _ in 0..10_000 {
            let a = regs.fifo_ready().read();
            if a.data_ready().bit() && a.done().bit() {
                let b = regs.fifo_ready().read();
                if b.data_ready().bit() && b.done().bit() {
                    pair = Some((
                        regs.fifo_data().read().value().bits(),
                        regs.fifo_data().read().value().bits(),
                    ));
                    break;
                }
            }
            delay.delay_us(1);
        }
        let (a, b) = pair.expect("raw pair timeout");
        super::emit(uart, b"WS53 TRNG PAIR ");
        hex(uart, a);
        hex(uart, b);
        super::emit(uart, b"\r\n");
    }
    // The experiment has excluded CPU consumers and observed the source pair
    // sequence. No production inter-core/service negotiation is inferred.
    let mut driver = unsafe { TrngDriver::from_configured(token) };
    let attempts = NonZeroU16::new(10_000).unwrap();
    for _ in 0..32 {
        let value = driver
            .read_word(attempts, &mut delay)
            .expect("HAL word timeout");
        assert!(value != 0 && value != u32::MAX);
        super::emit(uart, b"WS53 TRNG WORD ");
        hex(uart, value);
        super::emit(uart, b"\r\n");
    }
    for len in [0, 1, 3, 4, 5, 17] {
        let mut data = [0xa5; 19];
        driver
            .fill_bytes(&mut data[1..1 + len], attempts, &mut delay)
            .expect("fill timeout");
        assert_eq!(data[0], 0xa5);
        assert!(data[1 + len..].iter().all(|v| *v == 0xa5));
        super::emit(uart, b"WS53 TRNG FILL ");
        hex(uart, len as u32);
        for byte in data {
            super::emit(
                uart,
                &[
                    b"0123456789abcdef"[usize::from(byte >> 4)],
                    b"0123456789abcdef"[usize::from(byte & 15)],
                ],
            );
        }
        super::emit(uart, b"\r\n");
    }
    let token = driver.release_without_stopping();
    assert_eq!(ring.read().bits(), inherited);
    #[cfg(not(feature = "trng-ring-start-smoke"))]
    let mut attached = unsafe { TrngDriver::from_configured(token) };
    #[cfg(feature = "trng-ring-start-smoke")]
    let mut attached = {
        startup_word(4, u32::from(ring.read().bits()));
        let start = delay.0.current_value(TimerChannel::Channel1).unwrap();
        let driver = unsafe { TrngDriver::start_sdk_ring(token, &mut delay) }.unwrap();
        startup_word(
            6,
            start.wrapping_sub(delay.0.current_value(TimerChannel::Channel1).unwrap()),
        );
        startup_word(5, u32::from(ring.read().bits()));
        driver
    };
    for _ in 0..2 {
        let word = attached
            .read_word(attempts, &mut delay)
            .expect("reattach timeout");
        super::emit(uart, b"WS53 TRNG ATTACH ");
        hex(uart, word);
        super::emit(uart, b"\r\n");
    }
    #[cfg(feature = "ipc-random-smoke")]
    super::ipc_random_probe::check(&mut attached, &mut delay);
    let _token = attached.release_without_stopping();
    assert_eq!(ring.read().bits(), inherited);
    #[cfg(feature = "trng-ring-start-smoke")]
    {
        startup_word(7, 1);
        unsafe { core::arch::asm!("fence rw,rw", options(nostack)) };
        startup_word(0, 0x53544131);
    }
    delay.0.disable(TimerChannel::Channel1);
    super::emit(
        uart,
        b"WS53 PASS trng pairs=16 words=32 fills=6 attach=2 ring_preserved=1\r\n",
    );
}
