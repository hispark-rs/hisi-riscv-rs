//! Four retained buffers; IRQ wake, two simultaneous waiters and cancellation.
use super::*;
use core::{
    cell::Cell,
    future::Future,
    pin::Pin,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};
use critical_section::Mutex;
use hisi_hal::{dma, interrupt};
static WAKES: Mutex<[Cell<u32>; 4]> = Mutex::new([const { Cell::new(0) }; 4]);
static IRQS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static VTABLE: RawWakerVTable = RawWakerVTable::new(|p| raw(p as usize), notify, notify, |_| {});
fn raw(index: usize) -> RawWaker {
    RawWaker::new(index as *const (), &VTABLE)
}
fn notify(p: *const ()) {
    critical_section::with(|cs| {
        let slot = &WAKES.borrow(cs)[p as usize];
        slot.set(slot.get() + 1);
    });
}
fn wakes(index: usize) -> u32 {
    critical_section::with(|cs| WAKES.borrow(cs)[index].get())
}
#[unsafe(no_mangle)]
extern "C" fn M_DMA() {
    unsafe { dma::on_async_interrupt() };
    critical_section::with(|cs| IRQS.borrow(cs).set(IRQS.borrow(cs).get() + 1));
}
fn poll<F: Future + Unpin>(future: &mut F, index: usize) -> Poll<F::Output> {
    let waker = unsafe { Waker::from_raw(raw(index)) };
    Pin::new(future).poll(&mut Context::from_waker(&waker))
}
fn await_wake(index: usize, delay: &mut impl DelayNs) {
    for _ in 0..1000 {
        if wakes(index) > 0 {
            return;
        }
        delay.delay_us(1);
    }
    panic!("MDMA future did not wake");
}
fn raw_status() -> u32 {
    unsafe { &*ws53_pac::Mdma::PTR }
        .raw_interrupt_status()
        .read()
        .bits()
}
#[cfg(not(feature = "mdma-async-fault"))]
pub fn check(
    transfers: &mut [Option<Transfer<'_, Word>>; CHANNELS],
    results: &mut [u32; CHANNELS],
    delay: &mut impl DelayNs,
) -> bool {
    assert!(!riscv::register::mstatus::read().mie());
    interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
    let [a, b, c, d] = transfers;
    let (a, b, c, d) = (
        a.as_mut().unwrap(),
        b.as_mut().unwrap(),
        c.as_mut().unwrap(),
        d.as_mut().unwrap(),
    );
    // Dropping an unpolled future leaves a ready transfer reusable.
    drop(unsafe { a.run_async() }.unwrap());
    assert_eq!(raw_status(), 0);
    unsafe { riscv::interrupt::enable() };
    {
        let mut f = unsafe { a.run_async() }.unwrap();
        assert!(poll(&mut f, 0).is_pending());
        await_wake(0, delay);
        assert!(matches!(poll(&mut f,0),Poll::Ready(Ok(done)) if done.elements()==17));
    }
    a.acknowledge(dma::InterruptEvents::All).unwrap();
    // Two independently registered channels share one notification. Cancelling
    // one after wake must not destroy the other's raw completion or wake state.
    riscv::interrupt::disable();
    let mut fb = unsafe { b.run_async() }.unwrap();
    let mut fc = unsafe { c.run_async() }.unwrap();
    assert!(poll(&mut fb, 1).is_pending());
    assert!(poll(&mut fc, 2).is_pending());
    delay.delay_ms(2);
    assert_eq!(raw_status(), 6);
    unsafe { riscv::interrupt::enable() };
    await_wake(1, delay);
    await_wake(2, delay);
    drop(fb);
    assert_eq!(raw_status(), 6);
    assert!(matches!(poll(&mut fc,2),Poll::Ready(Ok(done)) if done.elements()==17));
    drop(fc);
    b.acknowledge(dma::InterruptEvents::All).unwrap();
    c.acknowledge(dma::InterruptEvents::All).unwrap();
    // Cancel a future while its completion is latched but delivery is blocked.
    // This does not claim physical cancellation of in-flight bus transactions.
    riscv::interrupt::disable();
    let mut fd = unsafe { d.run_async() }.unwrap();
    assert!(poll(&mut fd, 3).is_pending());
    delay.delay_ms(2);
    assert_eq!(raw_status(), 8);
    drop(fd);
    assert_eq!(raw_status(), 8);
    unsafe { riscv::interrupt::enable() };
    delay.delay_ms(2);
    assert_eq!(wakes(3), 0);
    d.acknowledge(dma::InterruptEvents::All).unwrap();
    for t in [a, b, c, d] {
        assert!(matches!(t.start(), Err(Error::NotReady)));
        assert!(matches!(unsafe { t.run_async() }, Err(Error::NotReady)));
        assert!(matches!(
            t.listen(dma::InterruptEvents::All),
            Err(Error::NotReady)
        ));
    }
    assert_eq!([wakes(0), wakes(1), wakes(2), wakes(3)], [1, 1, 1, 0]);
    assert!(critical_section::with(|cs| IRQS.borrow(cs).get()) >= 2);
    assert_eq!(raw_status(), 0);
    riscv::interrupt::disable();
    unsafe { interrupt::disable(interrupt::Interrupt::M_DMA) };
    results.fill(0);
    true
}

#[cfg(feature = "mdma-async-fault")]
pub fn fault_check(
    transfers: &mut [Option<Transfer<'_, Word>>; CHANNELS],
    results: &mut [u32; CHANNELS],
    delay: &mut impl DelayNs,
    uart: &Uart<'_, Uart2<'_>>,
) -> bool {
    assert!(!riscv::register::mstatus::read().mie());
    interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
    unsafe { riscv::interrupt::enable() };
    for (i, t) in transfers.iter_mut().enumerate() {
        let t = t.as_mut().unwrap();
        let mut f = unsafe { t.run_async() }.unwrap();
        assert!(poll(&mut f, i).is_pending());
        if i < 2 {
            await_wake(i, delay);
            assert!(matches!(poll(&mut f, i), Poll::Ready(Err(Error::Bus))));
            drop(f);
            assert_eq!(raw_status(), 1 << (i + 8));
            t.acknowledge(dma::InterruptEvents::All).unwrap();
        } else {
            delay.delay_ms(1);
            assert_eq!(wakes(i), 0);
            assert!(poll(&mut f, i).is_pending()); // permitted spurious executor poll
            drop(f);
            assert_eq!(raw_status(), 0);
            if i == 2 {
                // Model still has a posted write after cancellation. The host
                // verifies that fact before releasing its bus-stall input.
                super::super::emit(uart, b"WS53 MDMA ASYNC FAULT RELEASE POSTED\r\n");
                let r = unsafe { &*Uart2::ptr() };
                let mut budget = 100_000_000;
                while r.rx_fifo_cnt().read().bits() == 0 {
                    budget -= 1;
                    assert_ne!(budget, 0);
                }
                assert_eq!(r.rx().read().data().bits(), b'g');
                delay.delay_ms(1);
                assert_eq!(raw_status(), 0);
            }
        }
        assert!(matches!(unsafe { t.run_async() }, Err(Error::NotReady)));
    }
    assert_eq!([wakes(0), wakes(1), wakes(2), wakes(3)], [1, 1, 0, 0]);
    riscv::interrupt::disable();
    unsafe { interrupt::disable(interrupt::Interrupt::M_DMA) };
    results.fill(0);
    true
}
