//! One autonomous observation window per boot, before any peripheral waveform.
use hisi_hal::{
    interrupt,
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
};

const WAIT_TICKS: u32 = 30 * 32_000_000;

// Read-only diagnostic contract: phase, requested ticks, observed ticks.
// The host may inspect this in QEMU, but never supplies the elapsed value.
#[used]
#[unsafe(no_mangle)]
static mut WS53_FLASH_CAPTURE_WINDOW: [u32; 3] = [0; 3];

fn record(phase: u32, elapsed: u32) {
    // Publish the phase last so a stopped CPU cannot expose a new phase with
    // half-written fields. This is a diagnostic snapshot, not an IPC channel.
    unsafe {
        let words = core::ptr::addr_of_mut!(WS53_FLASH_CAPTURE_WINDOW).cast::<u32>();
        words.add(1).write_volatile(WAIT_TICKS);
        words.add(2).write_volatile(elapsed);
        riscv::asm::fence();
        words.write_volatile(phase);
    }
}

pub fn wait(timer: &mut TimerDriver<'_>) {
    let channel = TimerChannel::Channel1;
    assert!(!riscv::register::mstatus::read().mie());
    assert!(!interrupt::is_enabled(channel.interrupt()));
    timer.configure(
        channel,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(channel);
    let start = timer.current_value(channel).unwrap();
    record(1, 0);
    let mut last = start;
    let mut stalled = 0u32;
    // An instruction bound complements the independent elapsed-time deadline.
    // Do not WFI: a stopped clock or masked interrupt must not sleep forever.
    for _ in 0..1_000_000_000u32 {
        let now = timer.current_value(channel).unwrap();
        // With interrupts masked and no debugger intervention, a single poll
        // cannot legitimately consume 100ms. A disabled counter may read zero;
        // reject that discontinuity instead of mistaking it for elapsed time.
        assert!(
            last.wrapping_sub(now) <= 3_200_000,
            "capture-window counter jumped"
        );
        let elapsed = start.wrapping_sub(now);
        if elapsed >= WAIT_TICKS {
            timer.disable(channel);
            timer.clear_interrupt(channel);
            interrupt::clear_pending(channel.interrupt());
            record(2, elapsed);
            return;
        }
        stalled = if now == last { stalled + 1 } else { 0 };
        assert!(stalled < 1_000_000, "capture-window TIMER1 stopped");
        last = now;
    }
    panic!("capture-window TIMER1 progress too slow");
}
