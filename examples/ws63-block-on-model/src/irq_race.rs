//! Diagnostic only: the host injects a genuine modeled software IRQ at WFI.
use core::{
    future::poll_fn,
    sync::atomic::{AtomicU32, Ordering},
    task::Poll,
};
use hisi_hal::{
    asynch::{IrqSignal, block_on},
    peripherals::{SysCtl1, Uart0},
    software_interrupt::SoftwareInterrupt0,
    uart::Uart,
};

static SIGNAL: IrqSignal = IrqSignal::new();
// Single writer per field; host reads via qtest. No host writes to guest memory.
#[unsafe(no_mangle)]
static IRQ_RACE_STATE: [AtomicU32; 4] = [const { AtomicU32::new(0) }; 4];

#[unsafe(no_mangle)]
extern "C" fn SOFT_INT0() {
    IRQ_RACE_STATE[1].store(
        IRQ_RACE_STATE[1].load(Ordering::Relaxed) + 1,
        Ordering::Relaxed,
    );
    SoftwareInterrupt0::clear_interrupt();
    SIGNAL.signal();
}

#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn irq_race_wait() {
    block_on(poll_fn(|cx| {
        IRQ_RACE_STATE[0].store(
            IRQ_RACE_STATE[0].load(Ordering::Relaxed) + 1,
            Ordering::Relaxed,
        );
        if SIGNAL.take_fired() {
            return Poll::Ready(());
        }
        SIGNAL.register(cx.waker());
        Poll::Pending
    }));
}

#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn irq_race_done() {
    IRQ_RACE_STATE[2].store(1, Ordering::Relaxed);
    unsafe {
        core::arch::asm!("nop", options(nomem, nostack));
    }
}

pub fn run(token: SysCtl1<'static>, uart: &Uart<'_, Uart0<'_>>) {
    let _source = SoftwareInterrupt0::new(token);
    unsafe {
        hisi_hal::interrupt::enable_global();
    }
    let mstatus: u32;
    unsafe {
        core::arch::asm!("csrr {}, mstatus", out(reg) mstatus, options(nomem, nostack));
    }
    IRQ_RACE_STATE[3].store(mstatus, Ordering::Relaxed);
    uart.write(b"BLOCK_ON IRQ WINDOW: BEGIN\r\n");
    irq_race_wait();
    irq_race_done();
    uart.write(b"BLOCK_ON IRQ WINDOW: COMPLETE\r\n");
}
