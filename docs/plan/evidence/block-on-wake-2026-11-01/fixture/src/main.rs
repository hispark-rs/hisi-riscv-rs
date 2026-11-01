//! QEMU-only software-wake counterexample; not a hardware acceptance fixture.
#![no_std]
#![no_main]
use core::task::Poll;
use hisi_hal::{Peripherals, uart::{Uart, Config}};
use hisi_riscv_rt::entry;

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let uart = Uart::new_uart0(p.UART0, Config::default());
    unsafe { hisi_hal::interrupt::enable_global(); }
    uart.write(b"WAKE TEST: BEGIN\r\n");
    let mut second_poll = false;
    let future = core::future::poll_fn(|cx| {
        if second_poll {
            return Poll::Ready(());
        }
        second_poll = true;
        cx.waker().wake_by_ref();
        uart.write(b"WAKE TEST: WAKE ISSUED\r\n");
        Poll::Pending
    });
    #[cfg(not(feature = "poll-control"))]
    hisi_hal::asynch::block_on(future);
    // Diagnostic control only: two polls, no sleep; not an executor replacement.
    #[cfg(feature = "poll-control")]
    {
        use core::future::Future;
        let mut future = core::pin::pin!(future);
        let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
        assert!(future.as_mut().poll(&mut cx).is_pending());
        assert!(future.as_mut().poll(&mut cx).is_ready());
    }
    uart.write(b"WAKE TEST: COMPLETE\r\n");
    loop { core::hint::spin_loop(); }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop { core::hint::spin_loop(); }
}
