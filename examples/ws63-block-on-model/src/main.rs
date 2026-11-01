//! QEMU-only: software wakes must progress without any timer or software IRQ.
#![no_std]
#![no_main]

#[cfg(not(feature = "irq-race"))]
use core::{future::poll_fn, task::Poll};
#[cfg(not(feature = "irq-race"))]
use hisi_hal::asynch::block_on;
use hisi_hal::{
    Peripherals,
    uart::{Config, Uart},
};
use hisi_riscv_rt::entry;
#[cfg(feature = "irq-race")]
mod irq_race;

#[cfg(not(feature = "irq-race"))]
fn mie() -> bool {
    let value: usize;
    unsafe {
        core::arch::asm!("csrr {0}, mstatus", out(reg) value, options(nomem, nostack));
    }
    value & 8 != 0
}

#[allow(
    clippy::waker_clone_wake,
    reason = "test the consuming RawWaker wake callback on target"
)]
#[cfg(not(feature = "irq-race"))]
fn exercise() {
    let original_mie = mie();
    let mut polls = 0;
    let retained = block_on(poll_fn(|cx| {
        assert_eq!(mie(), original_mie);
        polls += 1;
        if polls == 4 {
            return Poll::Ready(cx.waker().clone());
        }
        cx.waker().wake_by_ref();
        cx.waker().clone().wake();
        Poll::Pending
    }));
    assert_eq!(polls, 4);
    assert_eq!(mie(), original_mie);
    retained.wake_by_ref(); // No active call; no pointer to the completed future.
    let mut repoll = false;
    block_on(poll_fn(|_| {
        if repoll {
            return Poll::Ready(());
        }
        repoll = true;
        retained.wake_by_ref(); // At most a spurious poll of the current future.
        Poll::Pending
    }));
    assert_eq!(mie(), original_mie);
}

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let uart = Uart::new_uart0(p.UART0, Config::default());
    #[cfg(feature = "irq-race")]
    irq_race::run(p.SYS_CTL1, &uart);
    #[cfg(not(feature = "irq-race"))]
    {
        unsafe {
            hisi_hal::interrupt::enable_global();
        }
        uart.write(b"BLOCK_ON SOFTWARE WAKE: BEGIN\r\n");
        exercise();
        uart.write(b"BLOCK_ON SOFTWARE WAKE: enabled=PASS\r\n");
        // Pure software futures do not need IRQ delivery; the helper must preserve MIE.
        // No WFI is entered while disabled: each Pending has a recorded software wake.
        unsafe {
            hisi_hal::interrupt::disable_global();
        }
        exercise();
        uart.write(b"BLOCK_ON SOFTWARE WAKE: disabled=PASS\r\n");
        unsafe {
            hisi_hal::interrupt::enable_global();
        }
        uart.write(b"BLOCK_ON SOFTWARE WAKE: PASS\r\n");
    }
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
