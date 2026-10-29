//! Independent Flash UART2 RX qualification, not a general clock initializer.
//! Requires the observed vendor FlashBoot32MHz UART/TIMER/pad handoff, mode1
//! memory and a quiescent control core. The binary never waits for a host ACK
//! to start. The peer supplies only functional RX test bytes after ARM markers.
#![no_std]
#![no_main]

use core::num::NonZeroU32;
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    Peripherals,
    gpio::Pad,
    interrupt,
    peripherals::Uart2,
    pinctrl::{Config, PadControl, Pull},
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::{
        DataBits, InterruptRouteError, LineConfig, Parity, SignalPath, StopDuration, Uart, UartPort,
    },
    wdt::{Config as WdtConfig, WatchdogDriver, WatchdogMode},
};
use hisi_riscv_rt::entry;

#[path = "../../ws53-flash/src/capture_delay.rs"]
mod capture_delay;
mod uart_async;

#[used]
static mut INITIALIZED: [u32; 3] = [0x53ac0123, 0x87654321, 0xfedcba98];
#[used]
static mut ZEROED: [u8; 257] = [0; 257];
#[used]
#[unsafe(no_mangle)]
static mut WS53_FLASH_UART_RX_PANIC: u32 = 0;
#[used]
#[unsafe(no_mangle)]
static mut WS53_FLASH_UART_RX_PHASE: u32 = 0;

fn phase(value: u32) {
    unsafe { core::ptr::addr_of_mut!(WS53_FLASH_UART_RX_PHASE).write_volatile(value) };
}

fn emit(uart: &Uart<'_, Uart2<'_>>, bytes: &[u8]) {
    for &byte in bytes {
        let mut budget = 1_000_000;
        while uart.try_write_byte(byte).is_err() {
            assert!(budget > 0, "UART TX stalled");
            budget -= 1;
        }
    }
}

// TIMER1 is independent of L0 reset and remains running through pad/line setup.
struct Delay<'a, 'd>(&'a mut TimerDriver<'d>);
impl DelayNs for Delay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let regs = unsafe { &*ws53_pac::Timer::PTR };
        assert!(!interrupt::is_enabled(TimerChannel::Channel1.interrupt()));
        assert!(regs.timer_control1().read().enable().bit());
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        let start = self.0.current_value(TimerChannel::Channel1).unwrap();
        let mut last = start;
        let mut stalled = 0;
        for _ in 0..10_000_000 {
            assert!(regs.timer_control1().read().enable().bit());
            let now = self.0.current_value(TimerChannel::Channel1).unwrap();
            assert!(last.wrapping_sub(now) <= 3_200_000, "TIMER1 jumped");
            stalled = if now == last { stalled + 1 } else { 0 };
            assert!(stalled < 1_000_000, "TIMER1 stalled");
            last = now;
            if start.wrapping_sub(now) >= ticks {
                return;
            }
        }
        panic!("TIMER1 timeout");
    }
}

// Two seconds of actual progressing TIMER1 time, not an instruction/PAUSE
// duration estimate. Reject stopped clocks and counter discontinuities too.
struct Deadline {
    start: u32,
    last: u32,
    stalled: u32,
}
impl Deadline {
    fn new(timer: &mut TimerDriver<'_>) -> Self {
        let start = timer.current_value(TimerChannel::Channel1).unwrap();
        Self {
            start,
            last: start,
            stalled: 0,
        }
    }
    fn expired(&mut self, timer: &mut TimerDriver<'_>) -> bool {
        assert!(
            unsafe { &*ws53_pac::Timer::PTR }
                .timer_control1()
                .read()
                .enable()
                .bit()
        );
        let now = timer.current_value(TimerChannel::Channel1).unwrap();
        assert!(
            self.last.wrapping_sub(now) <= 3_200_000,
            "RX deadline counter jumped"
        );
        self.stalled = if self.last == now {
            self.stalled + 1
        } else {
            0
        };
        assert!(self.stalled < 1_000_000, "RX deadline counter stalled");
        self.last = now;
        self.start.wrapping_sub(now) >= 64_000_000
    }
}

fn start_timer(timer: &mut TimerDriver<'_>) {
    let channel = TimerChannel::Channel1;
    assert!(!interrupt::is_enabled(channel.interrupt()));
    assert!(!interrupt::is_pending(channel.interrupt()));
    assert!(
        !unsafe { &*ws53_pac::Timer::PTR }
            .timer_control1()
            .read()
            .enable()
            .bit()
    );
    timer.configure(
        channel,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(channel);
}

fn prepare_pads(uart: &Uart<'_, Uart2<'_>>, delay: &mut Delay<'_, '_>) -> [PadControl<'static>; 2] {
    let irq = UartPort::Uart2.interrupt();
    let regs = unsafe { uart.register_block() };
    assert!(!interrupt::is_enabled(irq));
    assert!(!interrupt::is_pending(irq));
    assert_eq!(regs.intr_en().read().bits(), 0);
    // Keep the already configured boot console idle before taking its pads.
    let mut idle = false;
    for _ in 0..1000 {
        if uart.tx_flushed() {
            idle = true;
            break;
        }
        delay.delay_us(100);
    }
    assert!(idle, "boot UART TX busy");
    let glb = unsafe { &*ws53_pac::GlbCtl::PTR };
    assert!(glb.uart_soft_reset().read().uart_l0_reset_n().bit());
    // GPIO bank2 must not independently drive AGPIO1/2 during the mux change.
    assert_eq!(
        unsafe { &*ws53_pac::Gpio2::PTR }
            .gpio_sw_oen()
            .read()
            .bits()
            & 6,
        6
    );
    glb.uart_soft_reset()
        .modify(|_, w| w.uart_l0_reset_n().clear_bit());
    riscv::asm::fence();
    let asserted = !glb.uart_soft_reset().read().uart_l0_reset_n().bit();
    // No UART access while held. Always attempt release before propagating a
    // failed assertion or pad configuration; this is a serialized GLB handoff.
    let owners = if asserted {
        Some((|| {
            let mut tx = unsafe { PadControl::claim(Pad::Agpio1) }?;
            let mut rx = unsafe { PadControl::claim(Pad::Agpio2) }?;
            for owner in [&mut tx, &mut rx] {
                let before = owner.configuration()?;
                let cfg = Config::new(
                    owner
                        .pad()
                        .select_mode(hisi_hal::soc::ws53::pins::MuxMode::from_code(1).unwrap())
                        .unwrap(),
                    before.drive(),
                    Pull::Up,
                    true,
                    before.schmitt_enabled(),
                )?;
                owner.configure(cfg)?;
                if owner.configuration()? != cfg {
                    return Err(hisi_hal::pinctrl::Error::UnsupportedReadback);
                }
            }
            Ok::<_, hisi_hal::pinctrl::Error>([tx, rx])
        })())
    } else {
        None
    };
    glb.uart_soft_reset()
        .modify(|_, w| w.uart_l0_reset_n().set_bit());
    riscv::asm::fence();
    assert!(glb.uart_soft_reset().read().uart_l0_reset_n().bit());
    delay.delay_us(10);
    assert!(asserted, "L0 reset assertion failed");
    owners.unwrap().unwrap()
}

#[entry]
fn main() -> ! {
    assert!(!riscv::register::mstatus::read().mie());
    unsafe {
        assert_eq!(
            core::ptr::addr_of!(INITIALIZED).read_volatile(),
            [0x53ac0123, 0x87654321, 0xfedcba98]
        );
        for i in 0..257 {
            assert_eq!(
                (core::ptr::addr_of!(ZEROED) as *const u8)
                    .add(i)
                    .read_volatile(),
                0
            );
        }
    }
    assert_eq!(
        unsafe { &*ws53_pac::SystemMemory::PTR }
            .ccore_reset_status()
            .read()
            .bits(),
        0
    );
    let p = Peripherals::take().unwrap();
    // Attach only to the explicitly retained vendor FlashBoot handoff, then
    // replace pad/FIFO/line configuration below. No invented clock-gate bit.
    let mut uart = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    let mut timer = TimerDriver::new(unsafe { p.TIMER.assume_application_ownership() });
    start_timer(&mut timer);
    let cfg =
        WdtConfig::from_seconds(NonZeroU32::new(32000).unwrap(), 7, WatchdogMode::Reset).unwrap();
    let mut wdt = WatchdogDriver::new(unsafe { p.WDT.assume_application_ownership() }, cfg);
    wdt.stop(&mut Delay(&mut timer)).unwrap();
    timer.disable(TimerChannel::Channel1);
    phase(1);
    capture_delay::wait(&mut timer);
    start_timer(&mut timer);
    phase(2);
    let _pads = prepare_pads(&uart, &mut Delay(&mut timer));
    let line = LineConfig::new(
        NonZeroU32::new(32_000_000).unwrap(),
        115200,
        DataBits::Eight,
        Parity::None,
        StopDuration::One,
    )
    .unwrap();
    unsafe { uart.reset_and_configure(line, SignalPath::Pins, &mut Delay(&mut timer)) }.unwrap();
    // Refusals do not silently change existing notification enables.
    uart.set_receive_interrupts(true);
    assert_eq!(
        unsafe { uart.route_interrupt_to_application() },
        Err(InterruptRouteError::InterruptsEnabled)
    );
    assert_ne!(unsafe { uart.register_block() }.intr_en().read().bits(), 0);
    uart.set_receive_interrupts(false);
    let irq = UartPort::Uart2.interrupt();
    unsafe { interrupt::enable(irq) };
    assert_eq!(
        unsafe { uart.route_interrupt_to_application() },
        Err(InterruptRouteError::InterruptsEnabled)
    );
    assert!(interrupt::is_enabled(irq));
    unsafe { interrupt::disable(irq) };
    unsafe { uart.route_interrupt_to_application() }.unwrap();
    phase(3);
    interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
    emit(
        &uart,
        b"WS53 FLASH UART2 RX BEGIN clock_hz=32000000 baud=115200 pads=33,34 route=1 guards=2\r\n",
    );
    uart_async::check(&mut uart, &mut timer);
    phase(4);
    assert!(!interrupt::is_enabled(irq));
    assert!(!interrupt::is_pending(irq));
    assert_eq!(unsafe { uart.register_block() }.intr_en().read().bits(), 0);
    emit(&uart, b"WS53 FLASH UART2 RX PASS irq=42 rx=7 cancel=1\r\n");
    let mut flushed = false;
    for _ in 0..1000 {
        if uart.tx_flushed() {
            flushed = true;
            break;
        }
        Delay(&mut timer).delay_us(100);
    }
    assert!(flushed, "final UART flush timeout");
    timer.disable(TimerChannel::Channel1);
    timer.clear_interrupt(TimerChannel::Channel1);
    interrupt::clear_pending(TimerChannel::Channel1.interrupt());
    phase(5);
    loop {
        riscv::asm::wfi();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    riscv::interrupt::disable();
    unsafe {
        core::ptr::addr_of_mut!(WS53_FLASH_UART_RX_PANIC)
            .write_volatile(info.location().map_or(1, |p| p.line()))
    };
    loop {
        core::hint::spin_loop();
    }
}
