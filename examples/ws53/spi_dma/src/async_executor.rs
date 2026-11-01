use core::{
    cell::Cell,
    task::{RawWaker, RawWakerVTable, Waker},
};
use critical_section::Mutex;
use hisi_hal::timer::{TimerChannel, TimerDriver};
static WOKEN: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
static IRQS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static VTABLE: RawWakerVTable = RawWakerVTable::new(|_| raw(), |_| wake(), |_| wake(), |_| {});
fn raw() -> RawWaker {
    RawWaker::new(core::ptr::null(), &VTABLE)
}
fn wake() {
    critical_section::with(|cs| WOKEN.borrow(cs).set(true));
}
pub fn waker() -> Waker {
    unsafe { Waker::from_raw(raw()) }
}
pub fn take_wake() -> bool {
    critical_section::with(|cs| WOKEN.borrow(cs).replace(false))
}
pub fn irq_count() -> u32 {
    critical_section::with(|cs| IRQS.borrow(cs).get())
}
#[unsafe(no_mangle)]
extern "C" fn TIMER_0() {
    assert!(unsafe { TimerDriver::on_async_interrupt(TimerChannel::Channel0) });
    critical_section::with(|cs| IRQS.borrow(cs).set(IRQS.borrow(cs).get() + 1));
}
