//! Same-ELF physical/digital-peer async RX test. No polling of a Pending future
//! until its registered waker fires; bounded busy wait avoids assuming WFI semantics.
use core::{
    cell::Cell,
    future::Future,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};
use critical_section::Mutex;
use embedded_io_async::Read;
use hisi_hal::{
    interrupt,
    peripherals::Uart2,
    uart::{AsyncBindError, Uart, UartPort},
};

static WAKE: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
static COUNT: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static VTABLE: RawWakerVTable = RawWakerVTable::new(|_| raw(), |_| wake(), |_| wake(), |_| {});
fn raw() -> RawWaker {
    RawWaker::new(core::ptr::null(), &VTABLE)
}
fn wake() {
    critical_section::with(|cs| WAKE.borrow(cs).set(true));
}
fn count() -> u32 {
    critical_section::with(|cs| COUNT.borrow(cs).get())
}
#[unsafe(no_mangle)]
extern "C" fn UART_2() {
    unsafe { Uart::<Uart2<'static>>::on_async_receive_interrupt() };
    critical_section::with(|cs| COUNT.borrow(cs).set(COUNT.borrow(cs).get() + 1));
}
fn run<F: Future>(future: F, cx: &mut Context<'_>) -> F::Output {
    let mut future = core::pin::pin!(future);
    loop {
        critical_section::with(|cs| WAKE.borrow(cs).set(false));
        if let Poll::Ready(result) = future.as_mut().poll(cx) {
            return result;
        }
        let mut budget = 100_000_000;
        while !critical_section::with(|cs| WAKE.borrow(cs).get()) {
            budget -= 1;
            assert_ne!(budget, 0, "async read did not wake");
            core::hint::spin_loop();
        }
    }
}
pub fn check(uart: &mut Uart<'_, Uart2<'_>>) {
    let irq = UartPort::Uart2.interrupt();
    riscv::interrupt::disable();
    unsafe { interrupt::disable(irq) };
    uart.set_receive_interrupts(false);
    assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));
    // Refused takeover must preserve the existing enable state.
    uart.set_receive_interrupts(true);
    assert!(matches!(
        unsafe { uart.async_receiver() },
        Err(AsyncBindError::InterruptsEnabled)
    ));
    assert!(
        unsafe { uart.register_block() }
            .intr_en()
            .read()
            .rece_data_intr_en()
            .bit()
    );
    uart.set_receive_interrupts(false);
    unsafe { interrupt::enable(irq) };
    assert!(matches!(
        unsafe { uart.async_receiver() },
        Err(AsyncBindError::InterruptsEnabled)
    ));
    assert!(interrupt::is_enabled(irq));
    unsafe { interrupt::disable(irq) };
    // SAFETY: static vtable never dereferences its null payload; no allocation.
    let waker = unsafe { Waker::from_raw(raw()) };
    let mut cx = Context::from_waker(&waker);
    unsafe { riscv::interrupt::enable() };
    for (marker, expected) in [
        (&b"WS53 UART2 ASYNC ARM 0\r\n"[..], &b"\x00\xff\x51"[..]),
        (&b"WS53 UART2 ASYNC ARM 1\r\n"[..], &b"\xa5\x5a\x53"[..]),
    ] {
        let before = count();
        super::emit(uart, marker);
        let mut rx = unsafe { uart.async_receiver() }.unwrap();
        assert_eq!(run(rx.read(&mut []), &mut cx), Ok(0));
        assert!(!interrupt::is_enabled(irq));
        for &byte in expected {
            let mut buffer = [0xcc; 4];
            assert_eq!(run(rx.read(&mut buffer), &mut cx), Ok(1));
            assert_eq!(buffer, [byte, 0xcc, 0xcc, 0xcc]);
            assert!(!interrupt::is_enabled(irq));
            assert!(!interrupt::is_pending(irq));
        }
        assert!(
            count() > before,
            "stage did not exercise real wake delivery"
        );
    }
    // Cancel an actually Pending future, before the host is asked to send.
    let mut buffer = [0xcc; 4];
    {
        let mut rx = unsafe { uart.async_receiver() }.unwrap();
        {
            let mut future = core::pin::pin!(rx.read(&mut buffer));
            assert!(future.as_mut().poll(&mut cx).is_pending());
            assert!(interrupt::is_enabled(irq));
        }
        assert!(!interrupt::is_enabled(irq));
        assert!(!interrupt::is_pending(irq));
    }
    assert_eq!(buffer, [0xcc; 4]);
    let before = count();
    super::emit(uart, b"WS53 UART2 ASYNC CANCEL\r\n");
    let mut budget = 100_000_000;
    // Observation only: do not consume the byte or read-clear LSR.
    while unsafe { uart.register_block() }
        .rx_fifo_cnt()
        .read()
        .rx_fifo_level()
        .bits()
        == 0
    {
        budget -= 1;
        assert_ne!(budget, 0, "cancel input timeout");
        core::hint::spin_loop();
    }
    for _ in 0..10_000 {
        core::hint::spin_loop();
    }
    assert_eq!(count(), before);
    assert!(!interrupt::is_enabled(irq));
    assert_eq!(unsafe { uart.register_block() }.intr_en().read().bits(), 0);
    {
        let mut rx = unsafe { uart.async_receiver() }.unwrap();
        let mut future = core::pin::pin!(rx.read(&mut buffer));
        assert_eq!(future.as_mut().poll(&mut cx), Poll::Ready(Ok(1)));
    }
    assert_eq!(buffer, [0x3c, 0xcc, 0xcc, 0xcc]);
    assert_eq!(count(), before);
    riscv::interrupt::disable();
    assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));
    super::emit(
        uart,
        b"WS53 PASS uart2_async rx=7 wake=1 cancel=1 empty=1 guards=2\r\n",
    );
}
