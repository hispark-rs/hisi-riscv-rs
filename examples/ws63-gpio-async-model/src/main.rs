//! QEMU-only GPIO0 outputs 0/1 -> GPIO1 inputs 8/9 via model pinmux nets.
//! No hardware flashing/fixture qualification is implied.
#![no_std]
#![no_main]
use core::{
    cell::Cell,
    future::Future,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};
use critical_section::Mutex;
use embedded_hal_async::digital::Wait;
use hisi_hal::{
    Peripherals,
    gpio::{self, AnyPin, GpioBank, InputConfig, OutputConfig},
    interrupt::{self, Interrupt},
    uart::{Config, Uart},
};
use hisi_riscv_rt::entry;
static WAKES: Mutex<[Cell<u32>; 2]> = Mutex::new([Cell::new(0), Cell::new(0)]);
fn wake(data: *const ()) {
    critical_section::with(|cs| {
        let n = &WAKES.borrow(cs)[data as usize];
        n.set(n.get() + 1);
    });
}
fn count(n: usize) -> u32 {
    critical_section::with(|cs| WAKES.borrow(cs)[n].get())
}
static VT: RawWakerVTable =
    RawWakerVTable::new(|data| RawWaker::new(data, &VT), wake, wake, |_| {});
fn waker(n: usize) -> Waker {
    // SAFETY: n is an index 0/1, never dereferenced; callbacks refer to static storage.
    unsafe { Waker::from_raw(RawWaker::new(n as *const (), &VT)) }
}
fn deliver(n: usize, before: u32) {
    unsafe {
        interrupt::enable_global();
    }
    let mut budget = 1_000_000;
    while count(n) == before {
        budget -= 1;
        assert!(budget > 0, "GPIO did not wake");
    }
    unsafe {
        core::arch::asm!("csrci mstatus, 8");
    }
    assert_eq!(count(n), before + 1);
}
fn enabled() -> u32 {
    unsafe { &*ws63_pac::Gpio1::ptr() }
        .gpio_int_en()
        .read()
        .bits()
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
    assert_eq!(cause, 0x8000_0000 | Interrupt::GPIO_INT1 as u32);
    gpio::on_interrupt(GpioBank::Bank1);
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let uart = Uart::new_uart0(p.UART0, Config::default());
    uart.write(b"GPIO ASYNC: BEGIN\r\n");
    unsafe {
        core::arch::asm!("csrci mstatus, 8");
        core::arch::asm!("csrw mtvec, {0}", in(reg) atrap as *const () as usize);
        interrupt::init();
    }
    // Unique pins, exclusively owned by this QEMU fixture, with reset GPIO mux.
    let mut out0 = unsafe { AnyPin::steal(0) }.init_output(OutputConfig::default());
    let mut out1 = unsafe { AnyPin::steal(1) }.init_output(OutputConfig::default());
    let mut input0 = unsafe { AnyPin::steal(8) }.init_input(InputConfig::default());
    let mut input1 = unsafe { AnyPin::steal(9) }.init_input(InputConfig::default());
    let wa = waker(0);
    let wb = waker(1);
    let mut ca = Context::from_waker(&wa);
    let mut cb = Context::from_waker(&wb);
    drop(input0.wait_for_rising_edge());
    assert_eq!(enabled() & 3, 0);
    {
        let mut a = core::pin::pin!(input0.wait_for_rising_edge());
        let mut b = core::pin::pin!(input1.wait_for_rising_edge());
        assert!(a.as_mut().poll(&mut ca).is_pending());
        assert!(b.as_mut().poll(&mut cb).is_pending());
        gpio::on_interrupt(GpioBank::Bank1); // empty notification must not complete either
        assert!(a.as_mut().poll(&mut ca).is_pending());
        assert!(b.as_mut().poll(&mut cb).is_pending());
        assert_eq!((count(0), count(1)), (0, 0));
        out0.set_high();
        deliver(0, 0);
        assert_eq!(a.as_mut().poll(&mut ca), Poll::Ready(Ok(())));
        assert!(b.as_mut().poll(&mut cb).is_pending());
        assert_eq!(count(1), 0);
        assert_eq!(enabled() & 3, 2); // peer retained, only pin8 stopped
    } // cancels pending pin9
    assert_eq!(enabled() & 3, 0);
    out1.set_high();
    gpio::on_interrupt(GpioBank::Bank1);
    assert_eq!(count(1), 0);
    out1.set_low();
    {
        let mut b = core::pin::pin!(input1.wait_for_rising_edge());
        assert!(b.as_mut().poll(&mut cb).is_pending());
        out1.set_high();
        assert!(input_raw() & 2 != 0);
    } // cancellation after hardware latches, before ISR runs
    assert_eq!(enabled() & 3, 0);
    gpio::on_interrupt(GpioBank::Bank1);
    assert_eq!(count(1), 0);
    out0.set_low();
    out1.set_low();
    {
        let mut b = core::pin::pin!(input1.wait_for_rising_edge());
        assert!(b.as_mut().poll(&mut cb).is_pending());
        {
            let mut a = core::pin::pin!(input0.wait_for_rising_edge());
            assert!(a.as_mut().poll(&mut ca).is_pending());
            assert_eq!(enabled() & 3, 3);
            out1.set_high();
            assert_ne!(input_raw() & 2, 0);
        } // cancel pin8 while pin9 already has a latched event
        assert_eq!(enabled() & 3, 2);
        assert!(b.as_mut().poll(&mut cb).is_pending());
        deliver(1, 0);
        assert!(b.as_mut().poll(&mut cb).is_ready());
    }
    out0.set_high();
    {
        let mut high = core::pin::pin!(input0.wait_for_high());
        assert!(high.as_mut().poll(&mut ca).is_ready());
    }
    out0.set_low();
    {
        let mut low = core::pin::pin!(input0.wait_for_low());
        assert!(low.as_mut().poll(&mut ca).is_ready());
    }
    {
        let before = count(0);
        let mut f = core::pin::pin!(input0.wait_for_rising_edge());
        assert!(f.as_mut().poll(&mut ca).is_pending());
        out0.set_high();
        deliver(0, before);
        assert!(f.as_mut().poll(&mut ca).is_ready());
    }
    {
        let before = count(0);
        let mut f = core::pin::pin!(input0.wait_for_falling_edge());
        assert!(f.as_mut().poll(&mut ca).is_pending());
        out0.set_low();
        deliver(0, before);
        assert!(f.as_mut().poll(&mut ca).is_ready());
    }
    {
        let before = count(0);
        let mut f = core::pin::pin!(input0.wait_for_any_edge());
        assert!(f.as_mut().poll(&mut ca).is_pending());
        out0.set_high();
        deliver(0, before);
        assert!(f.as_mut().poll(&mut ca).is_ready());
    }
    assert_eq!(count(0), 4);
    assert_eq!(enabled() & 3, 0);
    uart.write(b"GPIO ASYNC: PASS simultaneous=2 spurious=1 cancels=3 immediate=2 edges=3\r\n");
    loop {
        core::hint::spin_loop();
    }
}
fn input_raw() -> u32 {
    unsafe { &*ws63_pac::Gpio1::ptr() }
        .gpio_int_raw()
        .read()
        .bits()
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
    let _ = core::fmt::write(&mut Diagnostic, format_args!("GPIO ASYNC: FAIL {info}\r\n"));
    loop {
        core::hint::spin_loop();
    }
}
