//! Flash-only GPIO17 async exercise, derived from the qualified RAM fixture.
//! Each weak-pull stimulus is logged before mutation and held for at least 1 ms.
//! No other bank client runs after the qualified vendor FlashBoot handoff.
use core::{
    cell::Cell,
    future::Future,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};
use critical_section::Mutex;
use embedded_hal::delay::DelayNs;
use embedded_hal_async::digital::Wait;
use hisi_hal::{
    gpio::{AsyncBindError, GpioBank, GpioDriver, InterruptTrigger, Pad},
    interrupt,
    peripherals::{Gpio0, Uart2},
    pinctrl::{Config, PadControl, Pull},
    soc::ws53::pins::MuxMode,
    timer::{TimerChannel, TimerDriver},
    uart::Uart,
};
const BIT: u32 = 1 << 17;
#[used]
#[unsafe(no_mangle)]
static mut WS53_FLASH_GPIO_STEPS: [[u32; 4]; 64] = [[0; 4]; 64];
#[used]
#[unsafe(no_mangle)]
static mut WS53_FLASH_GPIO_COUNTS: [u32; 4] = [0; 4];
static WAKE: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static IRQ: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static VTABLE: RawWakerVTable = RawWakerVTable::new(|_| raw(), |_| wake(), |_| wake(), |_| {});
fn raw() -> RawWaker {
    RawWaker::new(core::ptr::null(), &VTABLE)
}
fn wake() {
    critical_section::with(|cs| WAKE.borrow(cs).set(WAKE.borrow(cs).get() + 1));
}
fn counts() -> (u32, u32) {
    critical_section::with(|cs| (IRQ.borrow(cs).get(), WAKE.borrow(cs).get()))
}
#[unsafe(no_mangle)]
extern "C" fn GPIO_0() {
    assert!(unsafe { GpioDriver::on_async_interrupt(GpioBank::Bank0) });
    critical_section::with(|cs| IRQ.borrow(cs).set(IRQ.borrow(cs).get() + 1));
}
pub(super) fn cfg(pull: Pull) -> Config {
    let pad = Pad::Mgpio17;
    Config::new(
        pad.select_mode(MuxMode::from_code(0).unwrap()).unwrap(),
        pad.drive_setting(0).unwrap(),
        pull,
        true,
        true,
    )
    .unwrap()
}
fn gpio() -> &'static ws53_pac::gpio0::RegisterBlock {
    unsafe { &*Gpio0::ptr() }
}
fn drive_weak(high: bool) {
    // Fixture-only PAC stimulus: only weak pulls change, with all old users
    // halted. Input buffer/mux/drive/direction remain valid throughout the wait.
    // Do not create a second PadControl/AnyPin token beside the live input.
    let pads = unsafe { &*ws53_pac::PinmuxAon::ptr() };
    pads.mgpio17_pad()
        .modify(|_, w| unsafe { w.pull().bits(if high { 1 } else { 2 }) });
    let mut budget = 1_000_000;
    while (gpio().gpio_sw_out().read().gpio_sw_out().bits() & BIT != 0) != high {
        budget -= 1;
        assert_ne!(budget, 0, "pull did not reach input");
    }
}
fn quiet() {
    assert!(!interrupt::is_enabled(GpioBank::Bank0.interrupt()));
    assert!(!interrupt::is_pending(GpioBank::Bank0.interrupt()));
    assert_eq!(gpio().gpio_int_en().read().gpio_int_en().bits() & BIT, 0);
    assert_ne!(
        gpio().gpio_int_mask().read().gpio_int_mask().bits() & BIT,
        0
    );
    assert_eq!(gpio().gpio_intr().read().gpio_intr().bits() & BIT, 0);
}
fn pending() {
    let mut budget = 1_000_000;
    while gpio().gpio_intr().read().gpio_intr().bits() & BIT == 0 {
        budget -= 1;
        if budget == 0 {
            panic!("GPIO masked status did not assert");
        }
    }
}

// An inert execution marker for QEMU-only fault injection. It neither waits
// for a host nor alters a peripheral; silicon always executes the same code.
#[inline(never)]
#[unsafe(no_mangle)]
extern "C" fn WS53_FLASH_GPIO_ARMED() {
    riscv::asm::nop();
}
fn complete<F: Future<Output = Result<(), core::convert::Infallible>>>(
    future: F,
    cx: &mut Context<'_>,
    high: bool,
    wrong: Option<bool>,
    stimulus: &mut impl FnMut(bool),
) {
    riscv::interrupt::disable();
    let before = counts();
    {
        let mut f = core::pin::pin!(future);
        assert!(f.as_mut().poll(cx).is_pending());
        assert!(!unsafe { GpioDriver::on_async_interrupt(GpioBank::Bank0) });
        assert!(interrupt::is_enabled(GpioBank::Bank0.interrupt()));
        assert!(f.as_mut().poll(cx).is_pending());
        WS53_FLASH_GPIO_ARMED();
        if let Some(level) = wrong {
            stimulus(level);
            assert_eq!(gpio().gpio_intr().read().gpio_intr().bits() & BIT, 0);
            assert!(f.as_mut().poll(cx).is_pending());
        }
        stimulus(high);
        pending();
        unsafe { riscv::interrupt::enable() };
        let mut budget = 1_000_000;
        while counts().1 == before.1 {
            budget -= 1;
            assert_ne!(budget, 0, "GPIO waiter not woken");
        }
        riscv::interrupt::disable();
        assert_eq!(counts(), (before.0 + 1, before.1 + 1));
        // Completion remains sticky after the input reverses before re-poll.
        stimulus(!high);
        assert_eq!(f.as_mut().poll(cx), Poll::Ready(Ok(())));
    }
    quiet();
}
fn immediate<F: Future<Output = Result<(), core::convert::Infallible>>>(
    f: F,
    cx: &mut Context<'_>,
) {
    let before = counts();
    {
        let mut f = core::pin::pin!(f);
        assert_eq!(f.as_mut().poll(cx), Poll::Ready(Ok(())));
    }
    assert_eq!(counts(), before);
    quiet();
}
fn cancel_wait<F: Future<Output = Result<(), core::convert::Infallible>>>(
    f: F,
    cx: &mut Context<'_>,
    latched: bool,
    stimulus: &mut impl FnMut(bool),
) {
    riscv::interrupt::disable();
    let before = counts();
    {
        let mut f = core::pin::pin!(f);
        assert!(f.as_mut().poll(cx).is_pending());
        if latched {
            stimulus(true);
            pending();
        }
    }
    quiet();
    stimulus(false);
    stimulus(true);
    unsafe { riscv::interrupt::enable() };
    for _ in 0..10000 {
        core::hint::spin_loop();
    }
    riscv::interrupt::disable();
    assert_eq!(counts(), before);
    quiet();
}
pub fn check(
    bank: &mut GpioDriver<'_>,
    timer: &mut TimerDriver<'_>,
    uart: &Uart<'_, Uart2<'_>>,
    original: Config,
) -> u32 {
    riscv::interrupt::disable();
    let mut owner = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    owner.configure(cfg(Pull::Down)).unwrap();
    let pin = owner.into_gpio().unwrap_or_else(|_| super::fail());
    let mut input = bank.init_input(pin).unwrap_or_else(|_| super::fail());
    let peer_before = super::peer();
    assert_eq!(peer_before[2], 0, "bank still has another interrupt owner");
    unsafe { interrupt::enable(GpioBank::Bank0.interrupt()) };
    assert!(matches!(
        unsafe { input.async_wait() },
        Err(AsyncBindError::InterruptsEnabled)
    ));
    unsafe { interrupt::disable(GpioBank::Bank0.interrupt()) };
    input.listen(InterruptTrigger::BothEdges);
    assert!(matches!(
        unsafe { input.async_wait() },
        Err(AsyncBindError::InterruptsEnabled)
    ));
    input.unlisten();
    assert_eq!(counts(), (0, 0));
    let source = super::source();
    let mut steps = 0u32;
    let mut stimulus = |high: bool| {
        if super::source() != source {
            panic!("GPIO source changed before stimulus");
        }
        assert!(steps < 64);
        super::emit(uart, b"WS53 GPIO STEP ");
        super::number2(uart, steps);
        super::emit(uart, if high { b" 1\r\n" } else { b" 0\r\n" });
        let start = timer.current_value(TimerChannel::Channel1).unwrap();
        drive_weak(high);
        super::Delay(timer).delay_ms(1);
        let elapsed = start.wrapping_sub(timer.current_value(TimerChannel::Channel1).unwrap());
        assert!(elapsed >= 32_000);
        assert_eq!(super::source(), source, "GPIO source changed");
        unsafe {
            core::ptr::addr_of_mut!(WS53_FLASH_GPIO_STEPS)
                .cast::<[u32; 4]>()
                .add(steps as usize)
                .write_volatile([steps, u32::from(high), start, elapsed]);
        }
        steps += 1;
    };
    let waker = unsafe { Waker::from_raw(raw()) };
    let mut cx = Context::from_waker(&waker);
    {
        let mut input = unsafe { input.async_wait() }.unwrap();
        stimulus(true);
        immediate(input.wait_for_high(), &mut cx);
        stimulus(false);
        immediate(input.wait_for_low(), &mut cx);
        // Already at the requested edge's final level must NOT complete it.
        stimulus(true);
        complete(
            input.wait_for_rising_edge(),
            &mut cx,
            true,
            Some(false),
            &mut stimulus,
        );
        stimulus(false);
        complete(
            input.wait_for_falling_edge(),
            &mut cx,
            false,
            Some(true),
            &mut stimulus,
        );
        stimulus(false);
        complete(
            input.wait_for_any_edge(),
            &mut cx,
            true,
            None,
            &mut stimulus,
        );
        stimulus(true);
        complete(
            input.wait_for_any_edge(),
            &mut cx,
            false,
            None,
            &mut stimulus,
        );
        stimulus(false);
        complete(input.wait_for_high(), &mut cx, true, None, &mut stimulus);
        stimulus(true);
        complete(input.wait_for_low(), &mut cx, false, None, &mut stimulus);
        stimulus(false);
        cancel_wait(input.wait_for_rising_edge(), &mut cx, false, &mut stimulus);
        stimulus(false);
        complete(
            input.wait_for_rising_edge(),
            &mut cx,
            true,
            None,
            &mut stimulus,
        );
        stimulus(false);
        cancel_wait(input.wait_for_rising_edge(), &mut cx, true, &mut stimulus);
        stimulus(false);
        complete(
            input.wait_for_rising_edge(),
            &mut cx,
            true,
            None,
            &mut stimulus,
        );
    }
    quiet();
    assert_eq!(counts(), (8, 8));
    assert_eq!(super::peer(), peer_before);
    assert_eq!(steps, 35);
    super::emit(uart, b"WS53 FLASH GPIO RESTORE BEGIN\r\n");
    // A long low separator distinguishes possible pad restoration from stimuli.
    super::Delay(timer).delay_ms(20);
    let _ = input.free();
    let mut owner = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    owner.configure(original).unwrap();
    assert_eq!(owner.configuration().unwrap(), original);
    quiet();
    unsafe {
        core::ptr::addr_of_mut!(WS53_FLASH_GPIO_COUNTS).write_volatile([8, 8, steps, 1]);
    }
    super::emit(
        uart,
        b"WS53 PASS gpio_async pad=17 irq=34 wakes=8 immediate=2 cancels=2 guards=2 pad_config_restore=1\r\n",
    );
    steps
}
