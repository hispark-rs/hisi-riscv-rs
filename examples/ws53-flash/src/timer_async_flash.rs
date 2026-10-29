//! Autonomous observations of the shared Future fixture, never host-paced.
//! D5 pulses enclose main-context await/cancel observations, not precise ISR
//! latency. UART is sent while the output is low, before each measured window.
use hisi_hal::{
    gpio::Output,
    interrupt,
    peripherals::{Timer, Uart2},
    tcxo::TcxoDriver,
    timer::{TimerChannel, TimerDriver},
    uart::Uart,
};

// Published phase last: phase, finished stages, edges, expiries0/1,
// load0/control0-low4/raw0, load1/control1-low4/raw1, local enabled/pending masks,
// global MIE. Values after phase2 describe the async stage before later tests
// legitimately reconfigure TIMER. This is a stopped-debugger diagnostic only.
#[used]
#[unsafe(no_mangle)]
static mut WS53_FLASH_TIMER_ASYNC_STATE: [u32; 14] = [0; 14];

// phase1=before shared guards, phase2=high window active, phase3=window ended,
// phase4=all restored; channel/stage=u32::MAX before the first window. These
// are diagnostics for a stopped CPU, not a concurrent shared-memory protocol.
#[used]
#[unsafe(no_mangle)]
static mut WS53_FLASH_TIMER_ASYNC_STAGE: [u32; 5] = [0; 5];

// A QEMU-only fault runner may stop here to inject a stalled timer after all
// earlier checks. Normal execution never waits for a debugger or host action.
#[inline(never)]
#[unsafe(export_name = "WS53_FLASH_TIMER_ASYNC_READY")]
extern "C" fn observation_ready() {
    riscv::asm::nop();
}

fn publish_stage(phase: u32, channel: u32, stage: u32, stages: u32, edges: u32) {
    unsafe {
        let dst = core::ptr::addr_of_mut!(WS53_FLASH_TIMER_ASYNC_STAGE).cast::<u32>();
        for (index, word) in [channel, stage, stages, edges].into_iter().enumerate() {
            dst.add(index + 1).write_volatile(word);
        }
        riscv::asm::fence();
        dst.write_volatile(phase);
    }
}

fn flush_marker(uart: &Uart<'_, Uart2<'_>>) {
    // FIFO and shift register must drain while D5 remains low. Never wait
    // forever if the inherited UART clock/transmitter stops.
    for _ in 0..1_000_000 {
        if uart.tx_flushed() {
            return;
        }
    }
    panic!("timer async UART marker did not drain");
}

fn irq_masks() -> (u32, u32) {
    let mut enabled = 0;
    let mut pending = 0;
    for (index, ch) in [TimerChannel::Channel0, TimerChannel::Channel1]
        .into_iter()
        .enumerate()
    {
        enabled |= u32::from(interrupt::is_enabled(ch.interrupt())) << index;
        pending |= u32::from(interrupt::is_pending(ch.interrupt())) << index;
    }
    (enabled, pending)
}

fn state(phase: u32, stages: u32, edges: u32) -> [u32; 14] {
    let r = unsafe { &*Timer::ptr() };
    let (enabled, pending) = irq_masks();
    let counts = super::timer_async::counts();
    [
        phase,
        stages,
        edges,
        counts[0],
        counts[1],
        r.timer_load_count0().read().bits(),
        r.timer_control0().read().bits() & 15,
        r.timer_raw_intr0().read().bits(),
        r.timer_load_count1().read().bits(),
        r.timer_control1().read().bits() & 15,
        r.timer_raw_intr1().read().bits(),
        enabled,
        pending,
        u32::from(riscv::register::mstatus::read().mie()),
    ]
}

fn publish(words: [u32; 14]) {
    unsafe {
        let dst = core::ptr::addr_of_mut!(WS53_FLASH_TIMER_ASYNC_STATE).cast::<u32>();
        for (index, word) in words.into_iter().enumerate().skip(1) {
            dst.add(index).write_volatile(word);
        }
        riscv::asm::fence();
        dst.write_volatile(words[0]);
    }
}

pub fn check(
    timer: &mut TimerDriver<'_>,
    tcxo: &mut TcxoDriver<'_>,
    uart: &Uart<'_, Uart2<'_>>,
    output: &mut Output<'_, '_, '_>,
) {
    let before = state(1, 0, 0);
    if before[6] & 1 != 0
        || before[9] & 1 != 0
        || [before[7], before[10], before[11], before[12], before[13]] != [0; 5]
        || super::timer_async::dispatching()
        || super::timer_async::counts() != [0, 0]
        || output.is_set_high()
    {
        super::emit(uart, b"WS53 FAIL flash timer_async handoff\r\n");
        super::fail();
    }
    publish(before);
    publish_stage(1, u32::MAX, u32::MAX, 0, 0);
    super::emit(
        uart,
        b"WS53 FLASH TIMER ASYNC BEGIN channels=2 stages=14\r\n",
    );
    let mut stages = 0;
    let mut edges = 0;
    super::timer_async::check_with_observer(timer, tcxo, uart, |channel, stage, high| {
        assert_eq!(channel.index() * 7 + stage.index() as usize, stages);
        if high {
            assert_eq!(edges, stages * 2);
            assert!(!output.is_set_high());
            super::emit(uart, b"WS53 FLASH TIMER ASYNC ch=");
            super::emit(uart, &[b'0' + channel.index() as u8]);
            super::emit(uart, b" stage=");
            super::emit(uart, &[b'0' + stage.index()]);
            super::emit(uart, b" min_us=");
            super::emit(
                uart,
                match stage.minimum_us() {
                    1000 => b"1000",
                    3000 => b"3000",
                    5000 => b"5000",
                    6000 => b"6000",
                    20_000 => b"20000",
                    _ => unreachable!(),
                },
            );
            super::emit(uart, b"\r\n");
            flush_marker(uart);
            output.set_high();
        } else {
            assert_eq!(edges, stages * 2 + 1);
            assert!(output.is_set_high());
            output.set_low();
            stages += 1;
        }
        edges += 1;
        publish_stage(
            if high { 2 } else { 3 },
            channel.index() as u32,
            u32::from(stage.index()),
            stages as u32,
            edges as u32,
        );
        if high {
            observation_ready();
        }
    });
    assert_eq!((stages, edges), (14, 28));
    let after = state(2, stages as u32, edges as u32);
    assert_eq!(&after[3..5], &[8, 8]);
    assert_eq!((after[6], after[9]), (8, 8));
    assert_eq!(
        [after[7], after[10], after[11], after[12], after[13]],
        [0; 5]
    );
    assert!(!super::timer_async::dispatching());
    assert!(!output.is_set_high());
    publish(after);
    publish_stage(4, 1, 6, stages as u32, edges as u32);
    super::emit(
        uart,
        b"WS53 FLASH TIMER ASYNC restored=1 stages=14 gpio17_edges=28\r\n",
    );
}
