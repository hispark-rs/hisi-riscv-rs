//! Experimental vendor FlashBoot -> Rust fixture, not a whole-chip replacement.
//! UART2/32MHz TIMER clocks come from the qualified vendor FlashBoot, never
//! from debugger initialization. The runtime allocates mode1 RAM and leaves
//! the control CPU held in reset. MIO17 is connected only to DSLogic D5.
#![no_std]
#![no_main]

#[cfg(all(feature = "tsensor-smoke", feature = "timer-async-smoke"))]
compile_error!("Flash tsensor-smoke and timer-async-smoke require separate qualification profiles");

use core::{cell::Cell, num::NonZeroU32};
use critical_section::Mutex;
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    Peripherals,
    gpio::{GpioDriver, Pad},
    interrupt,
    peripherals::Uart2,
    pinctrl::{Config as PadConfig, PadControl, Pull},
    soc::ws53::pins::MuxMode,
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::Uart,
    wdt::{Config as WdtConfig, WatchdogDriver, WatchdogMode},
};
use hisi_riscv_rt::entry;

// One fixture implementation for RAM and Flash qualification. It never clears
// the shared epoch; the Flash handoff has kept the control CPU in reset and
// finished the vendor FlashBoot caller before these resources are claimed.
#[cfg(feature = "tcxo-snapshot-smoke")]
#[path = "../../ws53/ram_smoke/src/tcxo_snapshot.rs"]
mod tcxo_snapshot;
#[cfg(feature = "timer-async-smoke")]
#[path = "../../ws53/ram_smoke/src/timer_async.rs"]
mod timer_async;
#[cfg(feature = "timer-async-smoke")]
mod timer_async_flash;

#[cfg(feature = "capture-delay")]
mod capture_delay;
#[cfg(feature = "pwm0-smoke")]
#[path = "../../ws53/ram_smoke/src/pwm0.rs"]
mod pwm0;
#[cfg(feature = "pwm0-smoke")]
mod pwm_handoff;
#[cfg(feature = "rtc-smoke")]
mod rtc_handoff;
#[cfg(feature = "rtc-smoke")]
#[path = "../../ws53/ram_smoke/src/rtc_probe.rs"]
mod rtc_probe;
#[cfg(feature = "tsensor-smoke")]
mod tsensor_flash;
#[cfg(feature = "tsensor-smoke")]
#[path = "../../ws53/ram_smoke/src/tsensor_probe.rs"]
mod tsensor_probe;

#[cfg(feature = "tcxo-snapshot-smoke")]
#[used]
#[unsafe(no_mangle)]
static mut WS53_FLASH_PANIC: u32 = 0;

#[cfg(feature = "tcxo-snapshot-smoke")]
fn fail() -> ! {
    panic!("Flash TCXO fixture failed")
}

#[used]
static mut INITIALIZED: [u32; 3] = [0x53ac0123, 0x87654321, 0xfedcba98];
#[used]
static mut ZEROED: [u8; 257] = [0; 257];
static TICKS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));

fn ticks() -> u32 {
    critical_section::with(|cs| TICKS.borrow(cs).get())
}
fn emit(uart: &Uart<'_, Uart2<'_>>, data: &[u8]) {
    for &byte in data {
        let mut remaining = 1_000_000;
        while uart.try_write_byte(byte).is_err() {
            assert!(remaining > 0, "UART stuck");
            remaining -= 1;
        }
    }
}

fn gpio17<'a, 'd>(gpio: &'a mut GpioDriver<'d>) -> hisi_hal::gpio::Output<'a, 'd, 'static> {
    let pad = Pad::Mgpio17;
    let mut owner = unsafe { PadControl::claim(pad) }.unwrap();
    owner
        .configure(
            PadConfig::new(
                pad.select_mode(MuxMode::from_code(0).unwrap()).unwrap(),
                pad.drive_setting(0).unwrap(),
                Pull::None,
                true,
                true,
            )
            .unwrap(),
        )
        .unwrap();
    let pin = owner.into_gpio().unwrap_or_else(|_| panic!("GPIO route"));
    gpio.init_output(pin, false)
        .unwrap_or_else(|_| panic!("GPIO ownership"))
}

struct Delay<'a, 'd>(&'a mut TimerDriver<'d>);
impl DelayNs for Delay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let required = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        let start = self.0.current_value(TimerChannel::Channel1).unwrap();
        for _ in 0..10_000_000 {
            let now = self.0.current_value(TimerChannel::Channel1).unwrap();
            if start.wrapping_sub(now) >= required {
                return;
            }
        }
        panic!("TIMER clock stopped");
    }
}

#[entry]
fn main() -> ! {
    // Deliberately volatile: the compiler must not replace this startup test
    // with the constants in the Flash image or assume BSS was already zero.
    unsafe {
        assert_eq!(
            core::ptr::addr_of!(INITIALIZED).read_volatile(),
            [0x53ac0123, 0x87654321, 0xfedcba98]
        );
        for index in 0..257 {
            assert_eq!(
                (core::ptr::addr_of!(ZEROED) as *const u8)
                    .add(index)
                    .read_volatile(),
                0
            );
        }
    }
    assert_eq!(ticks(), 0);
    let p = Peripherals::take().unwrap();
    // The Flash runtime refused an active control core. Vendor FlashBoot has
    // finished and never returns, so its peripheral clients no longer run.
    #[allow(unused_mut)] // PWM reuses the RAM fixture's mutable UART interface.
    let mut uart = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    #[cfg(not(feature = "capture-delay"))]
    emit(&uart, b"WS53 FLASH data/bss OK\r\n");
    let mut timer = TimerDriver::new(unsafe { p.TIMER.assume_application_ownership() });
    timer.configure(
        TimerChannel::Channel1,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(TimerChannel::Channel1);
    // Vendor FlashBoot kicks this seven-second watchdog immediately before
    // entry. Stop it promptly, using a progressing independent timer delay.
    let config =
        WdtConfig::from_seconds(NonZeroU32::new(32000).unwrap(), 7, WatchdogMode::Reset).unwrap();
    let mut wdt = WatchdogDriver::new(unsafe { p.WDT.assume_application_ownership() }, config);
    wdt.stop(&mut Delay(&mut timer)).unwrap();
    assert!(
        !unsafe { &*ws53_pac::Wdt::PTR }
            .wdt_cr()
            .read()
            .wdt_en()
            .bit()
    );
    timer.disable(TimerChannel::Channel1);
    #[cfg(feature = "capture-delay")]
    {
        // The assertions above belong to this boot. Defer their first UART
        // report and the actual peripheral tests until USB can re-enumerate.
        // No UART RX, debugger action or host acknowledgement is required.
        capture_delay::wait(&mut timer);
        emit(
            &uart,
            b"WS53 FLASH observation delay_ms=30000 early_checks=retained\r\n",
        );
        emit(&uart, b"WS53 FLASH data/bss OK\r\n");
    }
    emit(&uart, b"WS53 FLASH watchdog stopped\r\n");

    // Only this opt-in profile needs D5 before PWM/RTC. The same bank and pad
    // tokens remain owned through the later legacy TIMER waveform.
    #[cfg(feature = "timer-async-smoke")]
    let mut gpio = GpioDriver::new_gpio0(unsafe { p.GPIO0.assume_application_ownership() });
    #[cfg(feature = "timer-async-smoke")]
    let mut output = gpio17(&mut gpio);

    #[cfg(feature = "tcxo-snapshot-smoke")]
    {
        interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
        let mut tcxo = hisi_hal::tcxo::TcxoDriver::new(
            unsafe { p.TCXO.assume_application_ownership() },
            core::num::NonZeroU16::new(1024).unwrap(),
        );
        tcxo_snapshot::check(&mut tcxo, &mut timer, &uart);
        #[cfg(feature = "tcxo-nonblocking-smoke")]
        tcxo_snapshot::check_nonblocking(&mut tcxo, &uart);
        #[cfg(feature = "timer-async-smoke")]
        {
            assert_eq!(ticks(), 0);
            timer_async_flash::check(&mut timer, &mut tcxo, &uart, &mut output);
            assert_eq!(ticks(), 0);
        }
        #[cfg(feature = "pwm0-smoke")]
        {
            if !pwm_handoff::idle_profile() {
                emit(&uart, b"WS53 FAIL flash pwm handoff\r\n");
                fail();
            }
            // Autonomous stage timing: a cold boot must not depend on UART RX
            // or a host sending the RAM fixture's stage acknowledgements.
            timer.configure(
                TimerChannel::Channel1,
                TimerMode::Periodic,
                TimerTicks::from_count(u32::MAX).unwrap(),
            );
            timer.enable(TimerChannel::Channel1);
            let mut delay = Delay(&mut timer);
            pwm0::check_with_stage(
                unsafe { p.PWM.assume_application_ownership() },
                tcxo,
                &mut uart,
                |uart, number| {
                    emit(uart, b"WS53 FLASH PWM STAGE ");
                    emit(uart, &[b'0' + number]);
                    emit(uart, b"\r\n");
                    delay.delay_ms(20);
                },
            );
            timer.disable(TimerChannel::Channel1);
        }
    }

    #[cfg(not(feature = "timer-async-smoke"))]
    let mut gpio = GpioDriver::new_gpio0(unsafe { p.GPIO0.assume_application_ownership() });
    #[cfg(not(feature = "timer-async-smoke"))]
    let mut output = gpio17(&mut gpio);
    #[cfg(feature = "rtc-smoke")]
    {
        if !rtc_handoff::idle_profile() {
            emit(&uart, b"WS53 FAIL flash rtc handoff\r\n");
            fail();
        }
        let mut edges = 0;
        let rtc_ok = rtc_probe::check_with_observer(
            unsafe { p.RTC0.assume_application_ownership() },
            unsafe { p.RTC1.assume_application_ownership() },
            &mut timer,
            &uart,
            |index, count| {
                assert_eq!(index * 4 + count as usize, edges + 1);
                edges += 1;
                if edges & 1 != 0 {
                    output.set_high();
                } else {
                    output.set_low();
                }
            },
        );
        assert!(rtc_ok, "RTC observations");
        assert_eq!(edges, 8);
        emit(&uart, b"WS53 FLASH RTC gpio17_edges=8 observer_us=32\r\n");
    }
    #[cfg(feature = "tsensor-smoke")]
    tsensor_flash::check(p.TSENSOR, &mut timer, &uart, &mut output);
    let channel = TimerChannel::Channel0;
    timer.configure(
        channel,
        TimerMode::Periodic,
        TimerTicks::from_micros(10_000).unwrap(),
    );
    interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
    unsafe {
        interrupt::enable(channel.interrupt());
        riscv::interrupt::enable();
    }
    timer.enable(channel);
    for edge in 1..=32 {
        let mut remaining = 100_000_000u32;
        loop {
            // Take one snapshot: an IRQ between two reads must not turn a
            // just-reached target into a false "missed notification" panic.
            let observed = ticks();
            if observed == edge {
                break;
            }
            assert!(observed < edge && remaining > 0, "IRQ sequence");
            remaining -= 1;
        }
        if edge & 1 != 0 {
            output.set_high();
        } else {
            output.set_low();
        }
    }
    riscv::interrupt::disable();
    timer.disable(channel);
    timer.clear_interrupt(channel);
    unsafe {
        interrupt::disable(channel.interrupt());
    }
    assert_eq!(ticks(), 32);
    emit(
        &uart,
        b"WS53 FLASH PASS irq=53 count=32 gpio17_edges=32 interval_us=10000\r\n",
    );
    loop {
        riscv::asm::wfi();
    }
}

#[unsafe(no_mangle)]
extern "C" fn TIMER_0() {
    #[cfg(feature = "timer-async-smoke")]
    if timer_async::dispatching() {
        timer_async::record(TimerChannel::Channel0);
        return;
    }
    unsafe {
        TimerDriver::acknowledge_interrupt(TimerChannel::Channel0);
    }
    critical_section::with(|cs| {
        let count = TICKS.borrow(cs);
        count.set(count.get() + 1);
    });
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    riscv::interrupt::disable();
    #[cfg(feature = "timer-async-smoke")]
    timer_async::capture_panic(_info);
    #[cfg(feature = "tcxo-snapshot-smoke")]
    unsafe {
        core::ptr::addr_of_mut!(WS53_FLASH_PANIC)
            .write_volatile(_info.location().map_or(1, |location| location.line()));
    }
    loop {
        core::hint::spin_loop();
    }
}
