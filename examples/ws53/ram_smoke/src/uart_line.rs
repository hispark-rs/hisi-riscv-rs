//! UART internal loopback and line readback; no pad or shared-clock writes.
use core::num::NonZeroU32;
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    peripherals::Uart2,
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::{DataBits, LineConfig, Parity, StopDuration, Uart, UartInstance, UartPort},
};
#[cfg(feature = "uart-clock-probe")]
#[path = "uart_clock.rs"]
mod clock;
#[cfg(feature = "uart-h0-init-smoke")]
#[path = "uart_h0_init.rs"]
mod h0_init;
#[cfg(any(feature = "uart-pad-probe", feature = "uart-h0-init-smoke"))]
#[path = "uart_pads.rs"]
mod pads;
#[cfg(all(
    feature = "uart-h0-init-smoke",
    any(
        feature = "uart-clock-probe",
        feature = "uart-pad-probe",
        feature = "uart-reset-probe",
        feature = "uart-l0-reset-probe"
    )
))]
compile_error!("H0 init needs its own unclaimed token and pad experiment");
#[cfg(all(
    feature = "uart-clock-probe",
    any(
        feature = "uart-pad-probe",
        feature = "uart-reset-probe",
        feature = "uart-l0-reset-probe"
    )
))]
compile_error!("clock observation needs exclusive UART/pad ownership");
#[cfg(all(
    feature = "uart-pad-probe",
    any(feature = "uart-reset-probe", feature = "uart-l0-reset-probe")
))]
compile_error!("run pad and reset observations in separate images");
#[cfg(feature = "uart-l0-reset-probe")]
#[path = "uart_l0_reset.rs"]
mod l0_reset;
#[cfg(feature = "uart-reset-probe")]
#[path = "uart_reset.rs"]
mod reset;
#[cfg(all(feature = "uart-l0-reset-probe", feature = "uart-reset-probe"))]
compile_error!("select L0 observation or H1/H0 observation, not both");

// RAM-only diagnostic retained even when UART cannot print after reconfiguration.
#[unsafe(no_mangle)]
pub static mut WS53_UART_LINE_STATE: [u32; 12] = [0; 12];
fn state(index: usize, value: u32) {
    unsafe {
        core::ptr::addr_of_mut!(WS53_UART_LINE_STATE)
            .cast::<u32>()
            .add(index)
            .write_volatile(value);
    }
}

struct Delay<'a, 'd>(&'a mut TimerDriver<'d>);
impl DelayNs for Delay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        let start = self.0.current_value(TimerChannel::Channel1).unwrap();
        for _ in 0..1_000_000 {
            if start.wrapping_sub(self.0.current_value(TimerChannel::Channel1).unwrap()) >= ticks {
                return;
            }
        }
        panic!("TIMER1 stopped");
    }
}
fn drain<T: UartInstance>(uart: &Uart<'_, T>, regs: &ws53_pac::uart0::RegisterBlock) {
    // Use the HAL, which retains receive errors consumed by LSR reads.
    for _ in 0..1_000_000 {
        if uart.tx_flushed() {
            return;
        }
    }
    state(11, regs.line_status().read().bits());
    panic!("UART TX did not drain");
}
fn prepare_loopback(regs: &ws53_pac::uart0::RegisterBlock) {
    unsafe {
        regs.intr_en().write_with_zero(|w| w);
        regs.fifo_ctl().write_with_zero(|w| {
            w.fifo_en()
                .set_bit()
                .rx_fifo_rst()
                .set_bit()
                .tx_fifo_rst()
                .set_bit()
        });
        regs.modem_ctl().write_with_zero(|w| w.lb_mode().set_bit());
    }
}
fn exercise<T: UartInstance>(
    uart: &mut Uart<'_, T>,
    timer: &mut TimerDriver<'_>,
    regs: &ws53_pac::uart0::RegisterBlock,
) -> [u32; 4] {
    state(0, 1);
    drain(uart, regs);
    state(0, 2);
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    let mut delay = Delay(timer);
    // Controlled RAM takeover: both vendor clients/IRQs stopped. Host sends no
    // input. Reset FIFOs before enabling internal loopback; original firmware is
    // restored by the runner's whole-board reset, not a saved UART context.
    prepare_loopback(regs);
    let mut cases = 0;
    let mut delayed_flushes = 0;
    let mut fifo_empty_while_busy = 0;
    for baud in [115200, 2_000_000, 4_000_000] {
        for data in [
            DataBits::Five,
            DataBits::Six,
            DataBits::Seven,
            DataBits::Eight,
        ] {
            for parity in [Parity::None, Parity::Even, Parity::Odd] {
                for stop in [
                    StopDuration::One,
                    StopDuration::OneAndHalf,
                    StopDuration::Two,
                ] {
                    let Some(config) = LineConfig::new(
                        NonZeroU32::new(32_000_000).unwrap(),
                        baud,
                        data,
                        parity,
                        stop,
                    ) else {
                        continue;
                    };
                    state(1, cases);
                    state(2, baud);
                    state(0, 3);
                    drain(uart, regs);
                    state(0, 4);
                    unsafe {
                        uart.configure_line(config, &mut delay);
                    }
                    let ctl = regs.uart_ctl().read();
                    state(3, ctl.bits());
                    state(4, regs.baud_ctl().read().bits());
                    state(5, regs.div_l().read().bits());
                    state(6, regs.div_h().read().bits());
                    state(7, regs.div_fra().read().bits());
                    state(0, 5);
                    assert!(!ctl.div_en().bit() && !ctl.xbreak().bit() && !ctl.sps().bit());
                    assert_eq!(
                        ctl.dlen().bits(),
                        match data {
                            DataBits::Five => 0,
                            DataBits::Six => 1,
                            DataBits::Seven => 2,
                            DataBits::Eight => 3,
                        }
                    );
                    assert_eq!(ctl.pen().bit(), parity != Parity::None);
                    assert_eq!(ctl.eps().bit(), parity == Parity::Even);
                    assert_eq!(ctl.stp().bit(), stop != StopDuration::One);
                    assert_eq!(
                        regs.baud_ctl().read().baud_div().bits() + 1,
                        config.samples_per_bit()
                    );
                    assert_eq!(
                        regs.baud_ctl().read().sample_phase().bits() + 1,
                        config.samples_per_bit() / 2
                    );
                    let divisor = u32::from(regs.div_l().read().div_l().bits())
                        | (u32::from(regs.div_h().read().div_h().bits()) << 8);
                    state(11, divisor);
                    // The original SDK get-baud helper opens DIV_EN before
                    // reading the integer latches. Record both access states.
                    regs.uart_ctl().modify(|_, w| w.div_en().set_bit());
                    let divisor = u32::from(regs.div_l().read().div_l().bits())
                        | (u32::from(regs.div_h().read().div_h().bits()) << 8);
                    state(5, divisor);
                    regs.uart_ctl().modify(|_, w| w.div_en().clear_bit());
                    let frac = u32::from(regs.div_fra().read().div_fra().bits());
                    assert_eq!(
                        32_000_000u64 * 64
                            / (u64::from(config.samples_per_bit())
                                * u64::from(divisor * 64 + frac)),
                        u64::from(config.actual_baud())
                    );
                    for byte in [0x15, 0x0a] {
                        state(8, u32::from(byte));
                        state(0, 6);
                        uart.try_write_byte(byte).unwrap();
                        let fifo_empty = regs.fifo_status().read().tx_fifo_empty().bit();
                        let flushed = embedded_hal_nb::serial::Write::flush(uart);
                        if baud == 115200 {
                            assert!(matches!(flushed, Err(nb::Error::WouldBlock)));
                            delayed_flushes += 1;
                            if fifo_empty {
                                fifo_empty_while_busy += 1;
                            }
                        }
                        // TX status polling must leave the valid loopback byte queued.
                        drain(uart, regs);
                        state(0, 7);
                        let mut received = None;
                        for _ in 0..100_000 {
                            match uart.read_byte() {
                                Ok(value) => {
                                    state(9, u32::from(value));
                                    received = Some(value);
                                    break;
                                }
                                Err(nb::Error::WouldBlock) => {}
                                Err(nb::Error::Other(_)) => {
                                    state(0, 8);
                                    panic!("loopback line error");
                                }
                            }
                        }
                        state(0, 9);
                        state(10, u32::from(received.is_some()));
                        assert_eq!(received, Some(byte));
                    }
                    assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 72);
    assert_eq!(delayed_flushes, 48);
    assert!(fifo_empty_while_busy > 0);
    state(0, 10);
    state(1, cases);
    drain(uart, regs);
    unsafe {
        uart.configure_line(
            LineConfig::new(
                NonZeroU32::new(32_000_000).unwrap(),
                115200,
                DataBits::Eight,
                Parity::None,
                StopDuration::One,
            )
            .unwrap(),
            &mut delay,
        );
        // Only the known UART2 console returns to its physical pads. Other
        // instances stay isolated in loopback until the whole-board reset.
        if T::PORT == UartPort::Uart2 {
            regs.modem_ctl().write_with_zero(|w| w);
        }
    }
    delay.0.disable(TimerChannel::Channel1);
    state(0, 11);
    [cases, delayed_flushes, cases * 2, fifo_empty_while_busy]
}
fn emit_result(uart: &Uart<'_, Uart2<'_>>) {
    super::emit(
        uart,
        b"WS53 PASS uart2_line cases=72 loopback_bytes=144 console_restored=1\r\n",
    );
    super::emit(
        uart,
        b"WS53 PASS uart2_flush delayed=48 rx_preserved=144 fifo_empty_gap=1\r\n",
    );
}

#[cfg(not(feature = "uart-all-lines-smoke"))]
pub fn check(uart: &mut Uart<'_, Uart2<'_>>, timer: &mut TimerDriver<'_>) {
    exercise(uart, timer, unsafe { &*ws53_pac::Uart2::PTR });
    emit_result(uart);
}

// Active port (3 means all complete), followed by (port, cases, delayed flushes,
// preserved bytes, FIFO-empty/busy observations) for each port. CPU writes only
// RAM here; the runner reads these objects after halting and before resetting.
#[cfg(feature = "uart-all-lines-smoke")]
#[unsafe(no_mangle)]
pub static mut WS53_UART_ALL_LINE_STATE: [u32; 16] = [0; 16];
#[cfg(feature = "uart-all-lines-smoke")]
fn all_state(index: usize, value: u32) {
    unsafe {
        core::ptr::addr_of_mut!(WS53_UART_ALL_LINE_STATE)
            .cast::<u32>()
            .add(index)
            .write_volatile(value)
    }
}
#[cfg(feature = "uart-all-lines-smoke")]
fn save_result(port: usize, result: [u32; 4]) {
    all_state(1 + port * 5, port as u32);
    for (index, value) in result.into_iter().enumerate() {
        all_state(2 + port * 5 + index, value);
    }
}
#[cfg(feature = "uart-all-lines-smoke")]
pub fn check_all(
    uart: &mut Uart<'_, Uart2<'_>>,
    timer: &mut TimerDriver<'_>,
    uart0: hisi_hal::peripherals::Uart0<'_>,
    #[cfg(not(feature = "uart-h0-init-smoke"))] uart1: hisi_hal::peripherals::Uart1<'_>,
    #[cfg(feature = "uart-h0-init-smoke")] uart1: hisi_hal::peripherals::Unclaimed<
        hisi_hal::peripherals::Uart1<'static>,
    >,
    #[cfg(feature = "uart-clock-probe")] gpio0: hisi_hal::peripherals::Gpio0<'_>,
) {
    // The runner stops vendor clients and checks H0's gate, reset release and
    // idle MDMA. H1/L0 gates and residual SDMA remain unqualified; this is a
    // bounded inherited-state experiment, not a cold-start or handoff proof.
    all_state(0, 0);
    let r0 = unsafe { &*ws53_pac::Uart0::PTR };
    prepare_loopback(r0);
    let mut u0 = unsafe { Uart::from_configured(uart0) };
    save_result(0, exercise(&mut u0, timer, r0));
    #[cfg(feature = "uart-pad-probe")]
    pads::check(&mut u0, timer, r0, 0);
    #[cfg(feature = "uart-reset-probe")]
    reset::check(&mut u0, timer, r0);
    all_state(0, 1);
    let r1 = unsafe { &*ws53_pac::Uart1::PTR };
    #[cfg(not(feature = "uart-h0-init-smoke"))]
    prepare_loopback(r1);
    #[cfg(not(feature = "uart-h0-init-smoke"))]
    let mut u1 = unsafe { Uart::from_configured(uart1) };
    #[cfg(feature = "uart-h0-init-smoke")]
    let mut u1 = h0_init::initialize(uart1, timer);
    save_result(1, exercise(&mut u1, timer, r1));
    #[cfg(feature = "uart-h0-init-smoke")]
    {
        #[cfg(not(feature = "uart-h0-init-pins"))]
        pads::check(&mut u1, timer, r1, 2);
        h0_init::finish();
    }
    #[cfg(feature = "uart-pad-probe")]
    {
        pads::check(&mut u1, timer, r1, 1);
        pads::check(&mut u1, timer, r1, 2);
    }
    #[cfg(feature = "uart-reset-probe")]
    reset::check(&mut u1, timer, r1);
    all_state(0, 2);
    save_result(2, exercise(uart, timer, unsafe { &*ws53_pac::Uart2::PTR }));
    #[cfg(feature = "uart-pad-probe")]
    pads::console(uart);
    #[cfg(feature = "uart-access-probe")]
    {
        let regs = unsafe { &*ws53_pac::Uart2::PTR };
        prepare_loopback(regs);
        timer.configure(
            TimerChannel::Channel1,
            TimerMode::Periodic,
            TimerTicks::from_count(u32::MAX).unwrap(),
        );
        timer.enable(TimerChannel::Channel1);
        reset::access::check(uart, timer, regs, 2);
        unsafe {
            regs.modem_ctl().write_with_zero(|w| w);
        }
        timer.disable(TimerChannel::Channel1);
    }
    #[cfg(feature = "uart-l0-reset-probe")]
    l0_reset::check(uart, timer);
    #[cfg(feature = "uart-clock-probe")]
    clock::check(&mut u0, &mut u1, timer, gpio0);
    all_state(0, 3);
    emit_result(uart);
    super::emit(
        uart,
        b"WS53 PASS uart_all_lines ports=3 cases=216 rx_preserved=432 delayed=144\r\n",
    );
    #[cfg(feature = "uart-clock-probe")]
    super::emit(
        uart,
        b"WS53 UART CLOCK OBSERVED ports=2 phases=6 restored=1\r\n",
    );
    #[cfg(feature = "uart-h0-init-smoke")]
    super::emit(
        uart,
        b"WS53 PASS h0_init constructor=1 guards=2 packet=264 restored=1\r\n",
    );
    #[cfg(feature = "uart-pad-probe")]
    super::emit(
        uart,
        b"WS53 PASS uart_pads routes=4 restored=8 tx_only=1\r\n",
    );
    #[cfg(feature = "uart-reset-probe")]
    reset::emit_results(uart);
    #[cfg(feature = "uart-l0-reset-probe")]
    l0_reset::emit_results(uart);
}
