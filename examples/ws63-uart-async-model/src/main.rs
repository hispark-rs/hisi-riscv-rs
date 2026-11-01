//! QEMU-only UART1 RX; UART0 diagnostics. No block_on/Embassy/board operations.
#![no_std]
#![no_main]
use core::{
    cell::Cell,
    future::Future,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};
use critical_section::Mutex;
use embedded_io_async::Read;
use hisi_hal::{
    Peripherals,
    interrupt::{self, Interrupt},
    uart::{self, Config, Uart, UartPort},
};
use hisi_riscv_rt::entry;
static WAKES: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
fn count() -> u32 {
    critical_section::with(|cs| WAKES.borrow(cs).get())
}
fn wake(_: *const ()) {
    critical_section::with(|cs| {
        let n = WAKES.borrow(cs);
        n.set(n.get() + 1);
    });
}
static VT: RawWakerVTable = RawWakerVTable::new(|d| RawWaker::new(d, &VT), wake, wake, |_| {});
fn enabled() -> bool {
    unsafe { &*ws63_pac::Uart1::ptr() }
        .intr_en()
        .read()
        .rece_data_intr_en()
        .bit_is_set()
}
fn ready() -> bool {
    unsafe { &*ws63_pac::Uart1::ptr() }
        .line_status()
        .read()
        .data_available()
        .bit_is_set()
}
fn wait_for(mut predicate: impl FnMut() -> bool) {
    let mut budget = 100_000_000u32;
    while !predicate() {
        budget -= 1;
        assert!(budget > 0, "UART peer/IRQ timeout");
    }
}
core::arch::global_asm!(
    ".section .text.atrap, \"ax\"",
    ".balign 4",
    ".global atrap",
    "atrap:",
    "    addi sp, sp, -64",
    "    sw ra,0(sp)",
    "    sw t0,4(sp)",
    "    sw t1,8(sp)",
    "    sw t2,12(sp)",
    "    sw t3,16(sp)",
    "    sw t4,20(sp)",
    "    sw t5,24(sp)",
    "    sw t6,28(sp)",
    "    sw a0,32(sp)",
    "    sw a1,36(sp)",
    "    sw a2,40(sp)",
    "    sw a3,44(sp)",
    "    sw a4,48(sp)",
    "    sw a5,52(sp)",
    "    sw a6,56(sp)",
    "    sw a7,60(sp)",
    "    call atrap_handle",
    "    lw ra,0(sp)",
    "    lw t0,4(sp)",
    "    lw t1,8(sp)",
    "    lw t2,12(sp)",
    "    lw t3,16(sp)",
    "    lw t4,20(sp)",
    "    lw t5,24(sp)",
    "    lw t6,28(sp)",
    "    lw a0,32(sp)",
    "    lw a1,36(sp)",
    "    lw a2,40(sp)",
    "    lw a3,44(sp)",
    "    lw a4,48(sp)",
    "    lw a5,52(sp)",
    "    lw a6,56(sp)",
    "    lw a7,60(sp)",
    "    addi sp, sp, 64",
    "    mret",
);

unsafe extern "C" {
    fn atrap();
}

#[unsafe(no_mangle)]
extern "C" fn atrap_handle() {
    let cause: u32;
    unsafe {
        core::arch::asm!("csrr {0}, mcause", out(reg) cause);
    }
    assert_eq!(cause, 0x8000_0000 | Interrupt::UART1_INT as u32);
    uart::on_interrupt(UartPort::Uart1);
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let console = Uart::new_uart0(p.UART0, Config::default());
    let mut rx = Uart::new_uart1(p.UART1, Config::default());
    unsafe {
        core::arch::asm!("csrci mstatus, 8");
        core::arch::asm!("csrw mtvec, {0}", in(reg) atrap as *const () as usize);
        interrupt::init();
        interrupt::enable(Interrupt::UART1_INT);
    }
    // Static callbacks never dereference the null payload; wake only schedules.
    let waker = unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VT)) };
    let mut cx = Context::from_waker(&waker);
    let mut buf = [0xa5];
    drop(Read::read(&mut rx, &mut buf));
    assert!(!enabled());
    {
        let mut read = core::pin::pin!(Read::read(&mut rx, &mut buf));
        assert!(read.as_mut().poll(&mut cx).is_pending());
        uart::on_interrupt(UartPort::Uart1); // Empty notification only schedules.
        assert_eq!(count(), 1);
        assert!(read.as_mut().poll(&mut cx).is_pending());
        console.write(b"UART ASYNC: ARM\r\n");
        unsafe {
            interrupt::enable_global();
        }
        wait_for(|| count() == 2);
        unsafe {
            core::arch::asm!("csrci mstatus, 8");
        }
        assert_eq!(read.as_mut().poll(&mut cx), Poll::Ready(Ok(1)));
    }
    assert_eq!(buf[0], 0);
    assert!(!enabled());
    for byte in [0xff, 0x53] {
        wait_for(ready);
        {
            let mut read = core::pin::pin!(Read::read(&mut rx, &mut buf));
            assert_eq!(read.as_mut().poll(&mut cx), Poll::Ready(Ok(1)));
        }
        assert_eq!(buf[0], byte);
    }
    {
        let mut read = core::pin::pin!(Read::read(&mut rx, &mut []));
        assert_eq!(read.as_mut().poll(&mut cx), Poll::Ready(Ok(0)));
    }
    buf[0] = 0xa5;
    {
        let mut read = core::pin::pin!(Read::read(&mut rx, &mut buf));
        assert!(read.as_mut().poll(&mut cx).is_pending());
    }
    assert!(!enabled() && buf[0] == 0xa5);
    console.write(b"UART ASYNC: CANCEL\r\n");
    wait_for(ready);
    assert_eq!(count(), 2);
    {
        let mut read = core::pin::pin!(Read::read(&mut rx, &mut buf));
        assert_eq!(read.as_mut().poll(&mut cx), Poll::Ready(Ok(1)));
    }
    assert_eq!(buf[0], 0x3c);
    {
        let mut read = core::pin::pin!(Read::read(&mut rx, &mut buf));
        assert!(read.as_mut().poll(&mut cx).is_pending());
        console.write(b"UART ASYNC: REARM\r\n");
        unsafe {
            interrupt::enable_global();
        }
        wait_for(|| count() == 3);
        unsafe {
            core::arch::asm!("csrci mstatus, 8");
        }
        assert_eq!(read.as_mut().poll(&mut cx), Poll::Ready(Ok(1)));
    }
    assert_eq!(buf[0], 0x7e);
    assert!(!enabled());
    uart::on_interrupt(UartPort::Uart1); // No stale waker after completion.
    assert_eq!(count(), 3);
    console.write(b"UART ASYNC: PASS irq=54 wakes=3 burst=3 cancel=1 empty=1\r\n");
    loop {
        core::hint::spin_loop();
    }
}
struct Diagnostic;
impl core::fmt::Write for Diagnostic {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        // Diagnostic PAC writes only; no duplicate HAL peripheral token.
        let r = unsafe { &*ws63_pac::Uart0::ptr() };
        for byte in text.bytes() {
            r.data().write(|w| unsafe { w.data().bits(byte) });
        }
        Ok(())
    }
}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    let _ = core::fmt::write(&mut Diagnostic, format_args!("UART ASYNC: FAIL {info}\r\n"));
    loop {
        core::hint::spin_loop();
    }
}
