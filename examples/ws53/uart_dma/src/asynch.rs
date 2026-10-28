//! QEMU-only: controlled peers and independent model drain checks, not HIL.
use super::*;
use core::{
    cell::Cell,
    future::Future,
    pin::Pin,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};
use critical_section::Mutex;
use hisi_hal::{
    dma::{self, uart::UartTransfer},
    interrupt,
    peripherals::{Uart0, Uart1},
};
static WOKEN: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
static DMA_IRQS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static LINE_IRQS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static VTABLE: RawWakerVTable = RawWakerVTable::new(|_| raw(), |_| wake(), |_| wake(), |_| {});
fn raw() -> RawWaker {
    RawWaker::new(core::ptr::null(), &VTABLE)
}
fn wake() {
    critical_section::with(|cs| WOKEN.borrow(cs).set(true));
}
fn count(slot: &Mutex<Cell<u32>>) -> u32 {
    critical_section::with(|cs| slot.borrow(cs).get())
}
#[unsafe(no_mangle)]
extern "C" fn M_DMA() {
    unsafe { dma::on_async_interrupt() };
    critical_section::with(|cs| DMA_IRQS.borrow(cs).set(DMA_IRQS.borrow(cs).get() + 1));
}
fn line<T: Instance>() {
    let id = unsafe { UartTransfer::<'_, T>::on_async_transmit_interrupt() };
    assert!(matches!(
        id,
        hisi_hal::uart::TransmitInterrupt::FifoAvailable
            | hisi_hal::uart::TransmitInterrupt::NoPending
    ));
    critical_section::with(|cs| LINE_IRQS.borrow(cs).set(LINE_IRQS.borrow(cs).get() + 1));
}
#[unsafe(no_mangle)]
extern "C" fn UART_0() {
    line::<Uart0<'static>>();
}
#[unsafe(no_mangle)]
extern "C" fn UART_1() {
    line::<Uart1<'static>>();
}
fn poll<F: Future + Unpin>(f: &mut F) -> Poll<F::Output> {
    critical_section::with(|cs| WOKEN.borrow(cs).set(false));
    let waker = unsafe { Waker::from_raw(raw()) };
    Pin::new(f).poll(&mut Context::from_waker(&waker))
}
fn finish<F: Future + Unpin>(f: &mut F, delay: &mut impl DelayNs) -> Option<F::Output> {
    // Test executor deadline; production API deliberately has no hidden timeout.
    for _ in 0..5000 {
        if critical_section::with(|cs| WOKEN.borrow(cs).get())
            && let Poll::Ready(result) = poll(f)
        {
            return Some(result);
        }
        delay.delay_us(1);
    }
    None
}
pub fn exercise<T: Instance>(
    t: &mut Option<UartTransfer<'static, T>>,
    rx: bool,
    scenario: u8,
    length: usize,
    console: &mut Uart<'_, Uart2<'_>>,
    delay: &mut impl DelayNs,
) {
    interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
    unsafe { riscv::interrupt::enable() };
    assert!(matches!(
        unsafe { t.as_mut().unwrap().transmitter_idle_async() },
        Err(Error::NotReady)
    ));
    drop(unsafe { t.as_mut().unwrap().run_async() }.unwrap()); // unpolled is inert
    let mut f = unsafe { t.as_mut().unwrap().run_async() }.unwrap();
    let first = poll(&mut f);
    if matches!(scenario, b'C' | b'B' | b'L') {
        match scenario {
            b'C' => assert_eq!(first, Poll::Ready(Err(Error::UartCapability))),
            b'B' => assert_eq!(first, Poll::Ready(Err(Error::UartBusy))),
            _ => assert!(matches!(first,Poll::Ready(Err(Error::UartLine(e))) if e.parity())),
        }
        drop(f);
    } else {
        assert!(first.is_pending());
        emit(console, b"WS53 UARTDMA armed\n");
        assert_eq!(receive(console), b'G');
        if matches!(scenario, b'D' | b'F' | b'I') {
            delay.delay_us(20);
            if scenario == b'F' {
                core::mem::forget(f);
                core::mem::forget(t.take().unwrap());
            } else {
                drop(f);
                if scenario == b'D' {
                    drop(t.take().unwrap());
                } else {
                    t.as_mut().unwrap().request_stop();
                }
            }
        } else {
            let result = finish(&mut f, delay);
            match scenario {
                b'N' | b'V' | b'E' => {
                    assert_eq!(result.unwrap().unwrap().elements(), length as u16)
                }
                b'R' | b'W' => assert_eq!(result, Some(Err(Error::Bus))),
                _ => assert!(result.is_none()),
            }
            drop(f); // executor timeout cancels the future, retaining all owners
            if matches!(scenario, b'N' | b'V' | b'E' | b'R' | b'W') {
                assert!(count(&DMA_IRQS) > 0);
            }
            if matches!(scenario, b'N' | b'V' | b'E') {
                if rx {
                    assert!(matches!(
                        unsafe { t.as_mut().unwrap().transmitter_idle_async() },
                        Err(Error::NotReady)
                    ));
                } else {
                    let mut idle = unsafe { t.as_mut().unwrap().transmitter_idle_async() }.unwrap();
                    assert!(poll(&mut idle).is_pending());
                    emit(console, b"WS53 UARTDMA accepted\n");
                    assert_eq!(receive(console), b'H');
                    let result = finish(&mut idle, delay);
                    if scenario == b'N' && result != Some(Ok(())) {
                        emit(
                            console,
                            if result.is_none() {
                                b"idle timeout\n"
                            } else {
                                b"idle error\n"
                            },
                        );
                        emit(
                            console,
                            if count(&LINE_IRQS) == 0 {
                                b"line irq zero\n"
                            } else {
                                b"line irq seen\n"
                            },
                        );
                        emit(
                            console,
                            if interrupt::is_enabled(T::PORT.interrupt()) {
                                b"line enabled\n"
                            } else {
                                b"line disabled\n"
                            },
                        );
                    }
                    if scenario == b'E' {
                        assert!(matches!(result,Some(Err(Error::UartLine(e))) if e.parity()));
                    } else {
                        assert_eq!(result, if scenario == b'N' { Some(Ok(())) } else { None });
                    }
                    drop(idle); // cancellation preserves all accepted TX bytes
                    if matches!(scenario, b'N' | b'E') {
                        assert!(count(&LINE_IRQS) > 0);
                    }
                }
            }
        }
    }
    if let Some(t) = t {
        assert!(matches!(unsafe { t.run_async() }, Err(Error::NotReady)));
    }
}
