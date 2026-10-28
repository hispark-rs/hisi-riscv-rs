//! Application/control users must be quiescent before the token is claimed.
//! No guessed sensor stop, READY-clear, IRQ, or common-clock operation.
use core::num::NonZeroU16;
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    peripherals::{Tsensor, Uart2},
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    tsensor::TempSensor,
    uart::Uart,
};

#[derive(Clone, Copy)]
#[repr(u32)]
pub enum Failure {
    TimerStopped = 1,
    Initialize = 2,
    FirstTimeout = 3,
    FirstTiming = 4,
    ReadSample = 5,
    Reattach = 6,
    Recover = 7,
    #[cfg(feature = "tsensor-restart-probe")]
    RestartState = 8,
    #[cfg(feature = "tsensor-restart-probe")]
    RestartTimeout = 9,
    FinalState = 10,
}

// Callbacks are observations in main context, outside critical sections. The
// default RAM caller is a no-op, retaining its existing UART protocol.
#[allow(dead_code)] // RAM uses the same observations with a no-op callback.
pub enum Observation {
    WindowStarted {
        window: u32,
    },
    FirstReady {
        initialization_ticks: u32,
        elapsed_ticks: u32,
        poll_delays: u32,
        code: u16,
    },
    Sample {
        index: usize,
        code: u16,
        nominal: u32,
    },
    #[cfg(feature = "tsensor-restart-probe")]
    RestartReady {
        phase: u32,
        elapsed_ticks: u32,
        snapshots: [u32; 5],
    },
    Complete {
        ready_reads: u32,
        attached_ready: bool,
        control: u32,
        passed: bool,
    },
    Failed(Failure),
}

struct Delay<'a, 'd, F> {
    timer: &'a mut TimerDriver<'d>,
    poll_delays: u32,
    observer: F,
}
impl<F: FnMut(Observation)> Delay<'_, '_, F> {
    fn now(&mut self) -> u32 {
        match self.timer.current_value(TimerChannel::Channel1) {
            Ok(value) => value,
            Err(_) => self.fail(Failure::TimerStopped),
        }
    }
    fn observe(&mut self, event: Observation) {
        (self.observer)(event);
    }
    fn fail(&mut self, failure: Failure) -> ! {
        self.timer.disable(TimerChannel::Channel1);
        self.observe(Observation::Failed(failure));
        panic!("temperature fixture failure");
    }
}
impl<F: FnMut(Observation)> DelayNs for Delay<'_, '_, F> {
    fn delay_ns(&mut self, ns: u32) {
        if ns == 10_000 {
            self.poll_delays += 1;
        }
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        let start = self.now();
        for _ in 0..10_000_000 {
            if start.wrapping_sub(self.now()) >= ticks {
                return;
            }
        }
        self.fail(Failure::TimerStopped);
    }
}
fn hex(uart: &Uart<'_, Uart2<'_>>, v: u32) {
    for shift in (0..8).rev() {
        super::emit(
            uart,
            &[b"0123456789abcdef"[((v >> (shift * 4)) & 15) as usize]],
        );
    }
}
#[allow(dead_code)] // Flash supplies the observer instead.
pub fn check(token: Tsensor<'_>, timer: &mut TimerDriver<'_>, uart: &Uart<'_, Uart2<'_>>) -> bool {
    check_with_observer(token, timer, uart, |_| {})
}
pub fn check_with_observer(
    token: Tsensor<'_>,
    timer: &mut TimerDriver<'_>,
    uart: &Uart<'_, Uart2<'_>>,
    observer: impl FnMut(Observation),
) -> bool {
    let regs = unsafe { &*ws53_pac::Tsensor::PTR };
    // RAM host preflight or autonomous Flash handoff has established this
    // uninitialized baseline. No status write or invented stop is permitted.
    assert_eq!(regs.control().read().value().bits(), 0);
    assert!(!regs.auto_status().read().ready().bit());
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    let mut delay = Delay {
        timer,
        poll_delays: 0,
        observer,
    };
    delay.observe(Observation::WindowStarted { window: 0 });
    let start = delay.now();
    let mut sensor = TempSensor::new(token).unwrap_or_else(|_| delay.fail(Failure::Initialize));
    let initialized = start.wrapping_sub(delay.now());
    let polls = NonZeroU16::new(10_000).unwrap();
    let first = sensor
        .read_bounded(polls, &mut delay)
        .unwrap_or_else(|_| delay.fail(Failure::FirstTimeout));
    let elapsed = start.wrapping_sub(delay.now());
    delay.observe(Observation::FirstReady {
        initialization_ticks: initialized,
        elapsed_ticks: elapsed,
        poll_delays: delay.poll_delays,
        code: first.raw(),
    });
    super::emit(uart, b"WS53 TSENSOR INIT ");
    hex(uart, initialized);
    hex(uart, delay.poll_delays);
    super::emit(uart, b"\r\n");
    super::emit(uart, b"WS53 TSENSOR FIRST ");
    hex(uart, elapsed);
    super::emit(uart, b"\r\n");
    if !(1..=3_200_000).contains(&elapsed) {
        delay.fail(Failure::FirstTiming);
    }
    for i in 0..12 {
        let code = if i == 0 {
            first
        } else {
            sensor
                .read_bounded(polls, &mut delay)
                .unwrap_or_else(|_| delay.fail(Failure::ReadSample))
        };
        let nominal = code
            .nominal_temperature()
            .map_or(0x8000_0000, |t| i32::from(t.centi_celsius()) as u32);
        delay.observe(Observation::Sample {
            index: i,
            code: code.raw(),
            nominal,
        });
        super::emit(uart, b"WS53 TSENSOR SAMPLE ");
        hex(uart, code.raw().into());
        hex(uart, nominal);
        super::emit(uart, b"\r\n");
        delay.delay_ms(10);
    }
    let mut ready_reads = 0;
    for _ in 0..32 {
        ready_reads += u32::from(
            sensor
                .try_read()
                .unwrap_or_else(|_| delay.fail(Failure::ReadSample))
                .is_some(),
        );
    }
    let token = sensor.release_without_stopping();
    let mut attached = TempSensor::new(token).unwrap_or_else(|_| delay.fail(Failure::Reattach));
    let attached_ready = attached
        .try_read()
        .unwrap_or_else(|_| delay.fail(Failure::Reattach))
        .is_some();
    attached
        .recover()
        .unwrap_or_else(|_| delay.fail(Failure::Recover));
    let control = regs.control().read().value().bits();
    super::emit(uart, b"WS53 TSENSOR STATE ");
    for v in [ready_reads, u32::from(attached_ready), control] {
        hex(uart, v);
    }
    super::emit(uart, b"\r\n");
    let _token = attached.release_without_stopping();
    #[cfg(feature = "tsensor-restart-probe")]
    {
        // Explicit diagnostic of source-defined START1, retaining the released
        // reservation. No stop/status/common-clock/power write is introduced.
        let mut probe = TempSensor::new(_token).unwrap_or_else(|_| delay.fail(Failure::Reattach));
        for phase in 0..2 {
            let snapshot = || {
                let s = regs.auto_status().read();
                (u32::from(s.ready().bit()) << 16) | u32::from(s.code().bits())
            };
            delay.observe(Observation::WindowStarted { window: phase + 1 });
            let mut state = [0u32; 5];
            state[0] = snapshot();
            let restart_start = delay.now();
            unsafe {
                regs.start().write_with_zero(|w| w.value().bits(1));
            }
            state[1] = snapshot();
            if probe.try_read() != Ok(None) {
                delay.fail(Failure::RestartState);
            }
            delay.delay_us(32);
            state[2] = snapshot();
            delay.delay_us(468);
            state[3] = snapshot();
            if probe.try_read() != Ok(None) {
                delay.fail(Failure::RestartState);
            }
            for _ in 0..200_000 {
                state[4] = snapshot();
                if state[4] & 0x10000 != 0 || restart_start.wrapping_sub(delay.now()) >= 3_200_000 {
                    break;
                }
            }
            let restart_ticks = restart_start.wrapping_sub(delay.now());
            if probe
                .try_read()
                .unwrap_or_else(|_| delay.fail(Failure::RestartState))
                .is_none()
                || !(16_000..=3_200_000).contains(&restart_ticks)
            {
                delay.fail(Failure::RestartTimeout);
            }
            if state.map(|s| s >> 16) != [1, 0, 0, 0, 1]
                || state[1..4].iter().any(|s| s & 0xffff != state[0] & 0xffff)
            {
                delay.fail(Failure::RestartState);
            }
            delay.observe(Observation::RestartReady {
                phase,
                elapsed_ticks: restart_ticks,
                snapshots: state,
            });
            super::emit(uart, b"WS53 TSENSOR RESTART ");
            hex(uart, phase);
            hex(uart, restart_ticks);
            for v in state {
                hex(uart, v);
            }
            super::emit(uart, b"\r\n");
        }
        let _token = probe.release_without_stopping();
        super::emit(uart, b"WS53 TSENSOR RESTART OBSERVED\r\n");
    }
    delay.timer.disable(TimerChannel::Channel1);
    let passed = ready_reads == 32 && attached_ready && control == 3;
    delay.observe(Observation::Complete {
        ready_reads,
        attached_ready,
        control,
        passed,
    });
    if passed {
        super::emit(
            uart,
            b"WS53 PASS tsensor samples=12 ready_reads=32 attach=1 control=3\r\n",
        );
    } else {
        super::emit(uart, b"WS53 FAIL tsensor observations\r\n");
        delay.observe(Observation::Failed(Failure::FinalState));
    }
    passed
}
