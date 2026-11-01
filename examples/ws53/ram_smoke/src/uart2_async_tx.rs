//! Async TX/flush and cancellation with the same UART-host protocol on HIL/QEMU.
use core::{
    cell::Cell,
    future::Future,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};
use critical_section::Mutex;
use embedded_io_async::Write;
use hisi_hal::{
    interrupt,
    peripherals::Uart2,
    uart::{AsyncBindError, TransmitInterrupt, Uart, UartPort},
};
static WOKEN: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
static IRQS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static VTABLE: RawWakerVTable = RawWakerVTable::new(|_| raw(), |_| wake(), |_| wake(), |_| {});
fn raw() -> RawWaker {
    RawWaker::new(core::ptr::null(), &VTABLE)
}
fn wake() {
    critical_section::with(|cs| WOKEN.borrow(cs).set(true));
}
fn count() -> u32 {
    critical_section::with(|cs| IRQS.borrow(cs).get())
}
fn regs() -> &'static ws53_pac::uart0::RegisterBlock {
    unsafe { &*Uart2::ptr() }
}
#[unsafe(no_mangle)]
extern "C" fn UART_2() {
    let id = unsafe { Uart::<Uart2<'static>>::on_async_transmit_interrupt() };
    assert!(matches!(
        id,
        TransmitInterrupt::FifoAvailable | TransmitInterrupt::NoPending
    ));
    critical_section::with(|cs| IRQS.borrow(cs).set(IRQS.borrow(cs).get() + 1));
}
fn run<F: Future>(future: F, cx: &mut Context<'_>) -> F::Output {
    let mut future = core::pin::pin!(future);
    loop {
        critical_section::with(|cs| WOKEN.borrow(cs).set(false));
        if let Poll::Ready(result) = future.as_mut().poll(cx) {
            return result;
        }
        let mut budget = 100_000_000;
        while !critical_section::with(|cs| WOKEN.borrow(cs).get()) {
            budget -= 1;
            assert_ne!(budget, 0, "TX future did not wake");
            core::hint::spin_loop();
        }
    }
}
fn flush(uart: &Uart<'_, Uart2<'_>>) {
    let mut budget = 100_000_000;
    while !uart.tx_flushed() {
        budget -= 1;
        assert_ne!(budget, 0, "TX did not drain");
    }
}
fn quiet() {
    assert_eq!(regs().intr_en().read().bits(), 0);
    assert!(!interrupt::is_enabled(UartPort::Uart2.interrupt()));
    assert!(!interrupt::is_pending(UartPort::Uart2.interrupt()));
}
pub fn check(uart: &mut Uart<'_, Uart2<'_>>) {
    riscv::interrupt::disable();
    uart.set_receive_interrupts(false);
    unsafe { interrupt::disable(UartPort::Uart2.interrupt()) };
    flush(uart);
    assert_eq!(regs().rx_fifo_cnt().read().bits(), 0);
    uart.set_receive_interrupts(true);
    assert!(matches!(
        unsafe { uart.async_transmitter() },
        Err(AsyncBindError::InterruptsEnabled)
    ));
    assert_eq!(regs().intr_en().read().bits(), 5);
    uart.set_receive_interrupts(false);
    regs().intr_en().modify(|_, w| w.ptim_en().set_bit());
    assert!(matches!(
        unsafe { uart.async_transmitter() },
        Err(AsyncBindError::InterruptsEnabled)
    ));
    assert_eq!(regs().intr_en().read().bits(), 16);
    regs().intr_en().modify(|_, w| w.ptim_en().clear_bit());
    unsafe { interrupt::enable(UartPort::Uart2.interrupt()) };
    assert!(matches!(
        unsafe { uart.async_transmitter() },
        Err(AsyncBindError::InterruptsEnabled)
    ));
    assert!(interrupt::is_enabled(UartPort::Uart2.interrupt()));
    unsafe { interrupt::disable(UartPort::Uart2.interrupt()) };
    let waker = unsafe { Waker::from_raw(raw()) };
    let mut cx = Context::from_waker(&waker);
    unsafe { riscv::interrupt::enable() };
    let before = count();
    super::emit(uart, b"WS53 UART2 ASYNC TX DATA ");
    flush(uart);
    {
        let mut tx = unsafe { uart.async_transmitter() }.unwrap();
        assert_eq!(run(tx.write(&[]), &mut cx), Ok(0));
        quiet();
        assert_eq!(run(tx.flush(), &mut cx), Ok(()));
        quiet();
        let payload: [u8; 257] = core::array::from_fn(|i| b'a' + (i % 26) as u8);
        run(tx.write_all(&payload), &mut cx).unwrap();
        run(tx.flush(), &mut cx).unwrap();
        quiet();
    }
    assert!(count() > before, "write/flush skipped real interrupts");
    assert!(uart.tx_flushed());
    super::emit(uart, b"\r\n");
    flush(uart);
    // Cancel a write while FIFO is full and halted. Its byte must never appear.
    super::emit(uart, b"WS53 UART2 TX CANCEL FULL ");
    flush(uart);
    regs().halt_tx().modify(|_, w| w.halt_tx().set_bit());
    for _ in 0..64 {
        assert!(uart.try_write_byte(b'C').is_ok());
    }
    let before = count();
    {
        let mut tx = unsafe { uart.async_transmitter() }.unwrap();
        {
            let mut future = core::pin::pin!(tx.write(b"?"));
            assert!(future.as_mut().poll(&mut cx).is_pending());
        }
        quiet();
    }
    assert_eq!(count(), before);
    assert_eq!(regs().tx_fifo_cnt().read().bits(), 64);
    regs().halt_tx().modify(|_, w| w.halt_tx().clear_bit());
    flush(uart);
    super::emit(uart, b"\r\n");
    flush(uart);
    // Cancel after expiry/request latched, with global delivery disabled.
    super::emit(uart, b"WS53 UART2 TX CANCEL LATCHED ");
    flush(uart);
    riscv::interrupt::disable();
    for _ in 0..64 {
        assert!(uart.try_write_byte(b'L').is_ok());
    }
    let before = count();
    {
        let mut tx = unsafe { uart.async_transmitter() }.unwrap();
        {
            let mut future = core::pin::pin!(tx.flush());
            assert!(future.as_mut().poll(&mut cx).is_pending());
            let mut budget = 100_000_000;
            while !interrupt::is_pending(UartPort::Uart2.interrupt()) {
                budget -= 1;
                assert_ne!(budget, 0, "TX request did not latch");
            }
        }
        quiet();
    }
    unsafe { riscv::interrupt::enable() };
    flush(uart);
    assert_eq!(count(), before);
    super::emit(uart, b"\r\n");
    flush(uart);
    // Dropped flush preserves a queued character, then a later flush completes.
    super::emit(uart, b"WS53 UART2 TX CANCEL FLUSH ");
    flush(uart);
    regs().halt_tx().modify(|_, w| w.halt_tx().set_bit());
    assert!(uart.try_write_byte(b'F').is_ok());
    {
        let mut tx = unsafe { uart.async_transmitter() }.unwrap();
        {
            let mut future = core::pin::pin!(tx.flush());
            assert!(future.as_mut().poll(&mut cx).is_pending());
        }
        quiet();
    }
    assert_eq!(regs().tx_fifo_cnt().read().bits(), 1);
    regs().halt_tx().modify(|_, w| w.halt_tx().clear_bit());
    {
        let mut tx = unsafe { uart.async_transmitter() }.unwrap();
        run(tx.flush(), &mut cx).unwrap();
        quiet();
    }
    super::emit(uart, b"\r\n");
    flush(uart);
    // RX data remains queued while TX flush reads read-clear LSR.
    super::emit(uart, b"WS53 UART2 TX RX ARM\r\n");
    flush(uart);
    let mut budget = 100_000_000;
    while regs().rx_fifo_cnt().read().bits() != 3 {
        budget -= 1;
        assert_ne!(budget, 0, "host RX timeout");
    }
    {
        let mut tx = unsafe { uart.async_transmitter() }.unwrap();
        run(tx.write_all(b"WS53 UART2 TX RX HELD\r\n"), &mut cx).unwrap();
        run(tx.flush(), &mut cx).unwrap();
        quiet();
    }
    for byte in [0, 255, 0x53] {
        assert_eq!(uart.read_byte(), Ok(byte));
    }
    assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));
    riscv::interrupt::disable();
    quiet();
    super::emit(
        uart,
        b"WS53 PASS uart2_async_tx bytes=257 cancel=3 guards=3 rx=3 flush=1 wake=1\r\n",
    );
}
