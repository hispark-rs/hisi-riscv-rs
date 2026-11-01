#![no_std]
use core::task::{Context, Poll};
use hisi_hal::asynch::IrqSignal;

pub fn wait(signal: &IrqSignal, cx: &Context<'_>) -> Poll<()> {
    if signal.take_fired() {
        Poll::Ready(())
    } else {
        signal.register(cx.waker());
        Poll::Pending
    }
}
