//! Autonomous, polling-only sensor fixture after vendor FlashBoot has returned
//! into the Rust runtime. No UART RX, sensor IRQ, STOP or WO readback is used.
use super::tsensor_probe::Observation;
use hisi_hal::{
    gpio::Output,
    interrupt,
    peripherals::{Tsensor, Uart2, Unclaimed},
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::Uart,
};

// Stopped-CPU diagnostics only: phase is published last, not an IPC protocol.
// phase/window/completed/edges/initial_control/initial_status/init_ticks/
// first_ticks/poll_delays/first_code/samples/ready_reads/attached/control/
// expected_windows/failure/final_status/timer1_control_low4.
#[used]
#[unsafe(no_mangle)]
static mut WS53_FLASH_TSENSOR_STATE: [u32; 18] = [0; 18];
// Three rows: [elapsed_ticks, snapshot0..4]. Snapshot = READY<<16 | CODE,
// distinct from AUTO_STATUS's register layout. First row has only snapshot4.
#[used]
#[unsafe(no_mangle)]
static mut WS53_FLASH_TSENSOR_WINDOWS: [u32; 18] = [0; 18];
#[used]
#[unsafe(no_mangle)]
static mut WS53_FLASH_TSENSOR_SAMPLES: [u32; 24] = [0; 24];

fn publish(state: &[u32; 18]) {
    unsafe {
        let dst = core::ptr::addr_of_mut!(WS53_FLASH_TSENSOR_STATE).cast::<u32>();
        for (index, value) in state.iter().copied().enumerate().skip(1) {
            dst.add(index).write_volatile(value);
        }
        riscv::asm::fence();
        dst.write_volatile(state[0]);
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
fn flush_marker(uart: &Uart<'_, Uart2<'_>>) {
    for _ in 0..1_000_000 {
        if uart.tx_flushed() {
            return;
        }
    }
    panic!("temperature UART marker did not drain");
}
fn snapshot(state: &mut [u32; 18]) {
    let sensor = unsafe { &*ws53_pac::Tsensor::PTR };
    let timer = unsafe { &*ws53_pac::Timer::PTR };
    state[13] = sensor.control().read().value().bits();
    state[16] = sensor.auto_status().read().bits();
    state[17] = timer.timer_control1().read().bits() & 15;
}
fn failure(uart: &Uart<'_, Uart2<'_>>, state: &mut [u32; 18], code: u32) {
    snapshot(state);
    report_failure(uart, state, code);
}
// Never read a peripheral whose shared domain has not been released. A failed
// release has unknown state; report using the independent application UART.
fn report_failure(uart: &Uart<'_, Uart2<'_>>, state: &mut [u32; 18], code: u32) {
    state[0] = 5;
    state[15] = code;
    publish(state);
    super::emit(uart, b"WS53 FAIL flash tsensor code=");
    hex(uart, code);
    super::emit(uart, b"\r\n");
}
fn window(index: u32, ticks: u32, snapshots: [u32; 5]) {
    unsafe {
        let dst = core::ptr::addr_of_mut!(WS53_FLASH_TSENSOR_WINDOWS)
            .cast::<u32>()
            .add(index as usize * 6);
        dst.write_volatile(ticks);
        for (index, value) in snapshots.into_iter().enumerate() {
            dst.add(index + 1).write_volatile(value);
        }
    }
}

pub fn check(
    token: Unclaimed<Tsensor<'_>>,
    timer: &mut TimerDriver<'_>,
    uart: &Uart<'_, Uart2<'_>>,
    output: &mut Output<'_, '_, '_>,
) {
    let regs = unsafe { &*ws53_pac::Tsensor::PTR };
    let timers = unsafe { &*ws53_pac::Timer::PTR };
    let system = unsafe { &*ws53_pac::SystemMemory::PTR };
    let expected = if cfg!(feature = "tsensor-restart-probe") {
        3
    } else {
        1
    };
    let mut state = [0u32; 18];
    state[1] = u32::MAX;
    // No sensor MMIO before a successful shared-domain release. These values
    // mark unavailable observations in a pre-release failure report.
    state[4] = u32::MAX;
    state[5] = u32::MAX;
    state[13] = u32::MAX;
    state[16] = u32::MAX;
    state[14] = expected;
    output.set_low();
    // The qualified runtime leaves the entire control domain held. Establish
    // independent TIMER/IRQ ownership before releasing its logic and clock.
    // The control CPU remains held throughout; no vendor control firmware runs.
    let timer_irqs_idle = [TimerChannel::Channel0, TimerChannel::Channel1]
        .into_iter()
        .all(|ch| !interrupt::is_enabled(ch.interrupt()) && !interrupt::is_pending(ch.interrupt()));
    if system.ccore_reset_status().read().bits() != 0
        || riscv::register::mstatus::read().mie()
        || timers.timer_control0().read().bits() & 1 != 0
        || timers.timer_control1().read().bits() & 1 != 0
        || timers.timer_raw_intr0().read().bits() != 0
        || timers.timer_raw_intr1().read().bits() != 0
        || !timer_irqs_idle
    {
        report_failure(uart, &mut state, 100);
        panic!("temperature Flash domain handoff rejected");
    }
    // TIMER1 is outside the control domain. Its inherited 32MHz clock has just
    // been exercised by TCXO/PWM/RTC; make it count before the SDK 5us settle.
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    // SAFETY: the qualified Flash runtime requires the entire control domain
    // held at entry; the vendor caller has returned and this single-hart fixture
    // has no domain/DMA/IRQ clients. Application RAM ownership is already fixed.
    // No other code releases the control CPU or resets this domain. Supply and
    // clocks must remain active; this lease does not grant a general DMA claim.
    let domain =
        unsafe { hisi_hal::control_domain::release_logic_and_clock(&mut super::Delay(timer)) };
    timer.disable(TimerChannel::Channel1);
    let _domain = match domain {
        Ok(lease) => lease,
        Err(_) => {
            report_failure(uart, &mut state, 101);
            panic!("temperature control domain release rejected");
        }
    };
    // Check the public domain and independent timer before any sensor access.
    // A failed domain check must not call failure(), which snapshots TSENSOR.
    if system.ccore_reset_status().read().bits() != 3
        || timers.timer_control1().read().bits() & 1 != 0
        || timers.timer_raw_intr1().read().bits() != 0
        || interrupt::is_pending(TimerChannel::Channel1.interrupt())
    {
        report_failure(uart, &mut state, 102);
        panic!("temperature Flash domain postconditions rejected");
    }
    // Keep the strict idle baseline and retain these actual observations on a
    // rejection; do not issue extra sensor reads while reporting that failure.
    state[4] = regs.control().read().value().bits();
    state[5] = regs.auto_status().read().bits();
    if state[4] != 0 || state[5] != 0 {
        state[13] = state[4];
        state[16] = state[5];
        report_failure(uart, &mut state, 103);
        panic!("temperature Flash sensor baseline rejected");
    }
    state[0] = 1;
    publish(&state);
    super::emit(uart, b"WS53 FLASH TSENSOR BEGIN windows=");
    super::emit(uart, &[b'0' + expected as u8]);
    super::emit(uart, b" control=");
    hex(uart, state[4]);
    super::emit(uart, b" status=");
    hex(uart, state[5]);
    super::emit(uart, b"\r\n");
    let mut high = false;
    let passed = super::tsensor_probe::check_with_observer(
        // The released logic/clock domain, idle sensor baseline, held control
        // CPU and completed vendor caller establish this fixture's handoff.
        // Its physical source clock still requires this candidate's HIL; no
        // generic DMA or other shared peripheral claim follows.
        unsafe { token.assume_application_ownership() },
        timer,
        uart,
        |event| match event {
            Observation::WindowStarted { window } => {
                assert!(!high && window == state[2] && window < expected);
                super::emit(uart, b"WS53 FLASH TSENSOR window=");
                super::emit(uart, &[b'0' + window as u8]);
                super::emit(uart, b" begin\r\n");
                flush_marker(uart);
                state[1] = window;
                state[0] = 2;
                state[3] += 1;
                publish(&state);
                output.set_high();
                high = true;
            }
            Observation::FirstReady {
                initialization_ticks,
                elapsed_ticks,
                poll_delays,
                code,
            } => {
                output.set_low();
                high = false;
                assert_eq!(state[1], 0);
                state[3] += 1;
                state[2] += 1;
                state[6] = initialization_ticks;
                state[7] = elapsed_ticks;
                state[8] = poll_delays;
                state[9] = code.into();
                window(0, elapsed_ticks, [0, 0, 0, 0, (1 << 16) | u32::from(code)]);
                state[0] = 3;
                publish(&state);
            }
            Observation::Sample {
                index,
                code,
                nominal,
            } => {
                assert_eq!(index, state[10] as usize);
                unsafe {
                    let dst = core::ptr::addr_of_mut!(WS53_FLASH_TSENSOR_SAMPLES)
                        .cast::<u32>()
                        .add(index * 2);
                    dst.write_volatile(code.into());
                    dst.add(1).write_volatile(nominal);
                }
                state[10] += 1;
                publish(&state);
            }
            #[cfg(feature = "tsensor-restart-probe")]
            Observation::RestartReady {
                phase,
                elapsed_ticks,
                snapshots,
            } => {
                output.set_low();
                high = false;
                assert_eq!(state[1], phase + 1);
                state[3] += 1;
                state[2] += 1;
                window(phase + 1, elapsed_ticks, snapshots);
                state[0] = 3;
                publish(&state);
            }
            Observation::Complete {
                ready_reads,
                attached_ready,
                control,
                passed,
            } => {
                state[11] = ready_reads;
                state[12] = attached_ready.into();
                state[13] = control;
                if !passed {
                    state[15] = 10;
                }
                publish(&state);
            }
            Observation::Failed(reason) => {
                if high {
                    output.set_low();
                    high = false;
                    state[3] += 1;
                }
                failure(uart, &mut state, reason as u32);
            }
        },
    );
    assert!(
        passed && !high && state[2] == expected && state[3] == expected * 2,
        "temperature Flash observations failed"
    );
    snapshot(&mut state);
    assert_eq!(state[17], 2, "temperature timer not stopped");
    assert_eq!(timers.timer_raw_intr1().read().bits(), 0);
    state[0] = 4;
    publish(&state);
    super::emit(uart, b"WS53 FLASH TSENSOR complete=1 windows=");
    super::emit(uart, &[b'0' + expected as u8]);
    super::emit(uart, b" gpio17_edges=");
    super::emit(uart, &[b'0' + (expected * 2) as u8]);
    super::emit(uart, b"\r\n");
}
