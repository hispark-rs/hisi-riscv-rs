//! Vendor FlashBoot -> Rust UART1/H0 constructor and immediate pad transfer.
//! Source/bus clocks are inherited, not supplied by a debugger. Control CPU
//! remains reset. This bounded experiment is not a production DMA-drain proof.
#![no_std]
#![no_main]
use core::{
    num::NonZeroU32,
    ptr::{addr_of, addr_of_mut},
};
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    Peripherals,
    gpio::{GpioBank, GpioDriver, Pad},
    interrupt,
    peripherals::{Uart1, Uart2},
    pinctrl::{Config, PadControl, Pull},
    soc::ws53::pins::MuxMode,
    timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks},
    uart::{DataBits, H0InitError, LineConfig, Parity, SignalPath, StopDuration, Uart, UartPort},
    wdt::{Config as WdtConfig, WatchdogDriver, WatchdogMode},
};
use hisi_riscv_rt::entry;
#[path = "../../ws53-flash/src/capture_delay.rs"]
mod capture_delay;

#[unsafe(no_mangle)]
pub static mut WS53_UART21: [u32; 64] = [0; 64];
#[unsafe(no_mangle)]
pub static mut WS53_UART21_DATA: u32 = 0x53a021;
#[unsafe(no_mangle)]
pub static mut WS53_UART21_BSS: u32 = 0;
fn put(i: usize, v: u32) {
    unsafe {
        addr_of_mut!(WS53_UART21)
            .cast::<u32>()
            .add(i)
            .write_volatile(v)
    }
}
fn snapshot() -> [u32; 64] {
    core::array::from_fn(|i| unsafe { addr_of!(WS53_UART21).cast::<u32>().add(i).read_volatile() })
}
fn fail(code: u32) -> ! {
    put(2, code);
    put(1, 0xee);
    ws53_uart21_failed()
}
#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn ws53_uart21_failed() -> ! {
    loop {
        unsafe { core::arch::asm!("nop", "nop", options(nomem, nostack)) }
    }
}
#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn ws53_uart21_done() -> ! {
    loop {
        unsafe { core::arch::asm!("nop", options(nomem, nostack)) }
    }
}
#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn ws53_uart21_ready() {
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
}
struct Delay<'a, 'd>(&'a mut TimerDriver<'d>);
impl DelayNs for Delay<'_, '_> {
    fn delay_ns(&mut self, ns: u32) {
        let ticks = (u64::from(ns) * 32_000_000).div_ceil(1_000_000_000) as u32;
        let start = self.0.current_value(TimerChannel::Channel1).unwrap();
        for _ in 0..1_000_000 {
            if start.wrapping_sub(self.0.current_value(TimerChannel::Channel1).unwrap()) >= ticks {
                return;
            }
        }
        fail(30)
    }
}
fn emit(uart: &Uart<'_, Uart2<'_>>, data: &[u8]) {
    for &byte in data {
        let mut n = 1_000_000;
        while uart.try_write_byte(byte).is_err() {
            n -= 1;
            if n == 0 {
                fail(31)
            }
        }
    }
}
fn hex(uart: &Uart<'_, Uart2<'_>>, value: u32) {
    for shift in (0..8).rev() {
        emit(
            uart,
            &[b"0123456789abcdef"[((value >> (shift * 4)) & 15) as usize]],
        )
    }
}
fn drain(uart: &Uart<'_, Uart1<'_>>) {
    for _ in 0..1_000_000 {
        if uart.tx_flushed() {
            return;
        }
    }
    fail(32)
}
fn packet(uart: &Uart<'_, Uart1<'_>>, route: u8) {
    for byte in [0x55, 0xaa, b'W', b'S', b'5', b'3', route, 0xc3]
        .into_iter()
        .chain(0..=255u8)
    {
        uart.try_write_byte(byte).unwrap();
        drain(uart);
    }
}
// No raw pad numbers: only the two previously measured UART1 mode1 pads.
fn pad(pad: Pad) -> (PadControl<'static>, Config, bool) {
    assert!(matches!(pad, Pad::Mgpio8 | Pad::Mgpio9));
    let mask = pad.gpio_line().unwrap().mask();
    let gpio = unsafe { &*ws53_pac::Gpio0::PTR };
    let old = gpio.gpio_sw_oen().read().pins().bits();
    gpio.gpio_sw_oen()
        .modify(|r, w| unsafe { w.pins().bits(r.pins().bits() | mask) });
    let mut owner = unsafe { PadControl::claim(pad) }.unwrap();
    let saved = owner.configuration().unwrap();
    owner
        .configure(
            Config::new(
                pad.select_mode(MuxMode::from_code(1).unwrap()).unwrap(),
                pad.drive_setting(0).unwrap(),
                Pull::Up,
                true,
                saved.schmitt_enabled(),
            )
            .unwrap(),
        )
        .unwrap();
    (owner, saved, old & mask != 0)
}
fn restore((mut owner, saved, input): (PadControl<'_>, Config, bool)) {
    let mask = owner.pad().gpio_line().unwrap().mask();
    owner.configure(saved).unwrap();
    assert_eq!(owner.configuration().unwrap(), saved);
    let gpio = unsafe { &*ws53_pac::Gpio0::PTR };
    gpio.gpio_sw_oen().modify(|r, w| unsafe {
        let old = r.pins().bits();
        w.pins().bits(if input { old | mask } else { old & !mask })
    });
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    put(0, 0x53f02101);
    put(3, unsafe { addr_of!(WS53_UART21_DATA).read_volatile() });
    put(4, unsafe { addr_of!(WS53_UART21_BSS).read_volatile() });
    let memory = unsafe { &*ws53_pac::SystemMemory::PTR };
    for (i, v) in [
        memory.ccore_reset_status().read().bits(),
        memory.bram_share().read().bits(),
        memory.dtcm_share().read().bits(),
        unsafe { &*ws53_pac::Mdma::PTR }
            .enabled_channels()
            .read()
            .bits(),
        memory.platform_enable().read().bits(),
        u32::from(riscv::register::mstatus::read().mie()),
    ]
    .into_iter()
    .enumerate()
    {
        put(5 + i, v)
    }
    if snapshot()[3..11] != [0x53a021, 0, 0, 0x3f8, 0x3c, 0, 0, 0] {
        fail(1)
    }
    let console = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    let mut timer = TimerDriver::new(unsafe { p.TIMER.assume_application_ownership() });
    let ch = TimerChannel::Channel1;
    let irq = UartPort::Uart1.interrupt();
    if interrupt::is_enabled(ch.interrupt())
        || interrupt::is_pending(ch.interrupt())
        || interrupt::is_enabled(irq)
        || interrupt::is_pending(irq)
    {
        fail(2)
    }
    timer.configure(
        ch,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(ch);
    let wd =
        WdtConfig::from_seconds(NonZeroU32::new(32000).unwrap(), 7, WatchdogMode::Reset).unwrap();
    WatchdogDriver::new(unsafe { p.WDT.assume_application_ownership() }, wd)
        .stop(&mut Delay(&mut timer))
        .unwrap();
    put(11, unsafe { &*ws53_pac::Wdt::PTR }.wdt_cr().read().bits());
    if snapshot()[11] & 1 != 0 {
        fail(3)
    }
    let _logic =
        unsafe { hisi_hal::control_domain::release_logic_and_clock(&mut Delay(&mut timer)) }
            .unwrap();
    put(12, memory.ccore_reset_status().read().bits());
    if snapshot()[12] != 3 {
        fail(4)
    }
    timer.disable(ch);
    timer.clear_interrupt(ch);
    interrupt::clear_pending(ch.interrupt());
    put(1, 1);
    ws53_uart21_ready();
    capture_delay::wait(&mut timer);
    let gpio = unsafe { &*ws53_pac::Gpio0::PTR };
    let directions = gpio.gpio_sw_oen().read().bits();
    let mut marker_pad = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    let original = marker_pad.configuration().unwrap();
    if original.mode().mode().code() != 0
        || interrupt::is_enabled(GpioBank::Bank0.interrupt())
        || gpio.gpio_int_en().read().bits() != 0
        || gpio.gpio_sw_ctl().read().bits() != 0
        || directions & (1 << 17) == 0
    {
        fail(5)
    }
    marker_pad
        .configure(
            Config::new(
                original.mode(),
                Pad::Mgpio17.drive_setting(0).unwrap(),
                Pull::Down,
                true,
                original.schmitt_enabled(),
            )
            .unwrap(),
        )
        .unwrap();
    let pin = marker_pad.into_gpio().unwrap_or_else(|_| fail(6));
    let tx = pad(Pad::Mgpio8);
    let rx = pad(Pad::Mgpio9);
    let mut bank = GpioDriver::new_gpio0(unsafe { p.GPIO0.assume_application_ownership() });
    let mut marker = bank.init_output(pin, false).unwrap_or_else(|_| fail(7));
    let crg = unsafe { &*ws53_pac::MCrg::PTR };
    let regs = unsafe { &*ws53_pac::Uart1::PTR };
    let old = crg.m_clken1().read().bits();
    let reset = crg.uart_soft_reset().read().bits();
    put(13, u32::from(old));
    put(16, u32::from(reset));
    if old & 16 == 0 || reset & 1 == 0 {
        fail(8)
    }
    if regs.intr_en().read().bits() != 0 || !regs.line_status().read().tx_empty_s().bit() {
        fail(9)
    }
    let cfg = LineConfig::new(
        NonZeroU32::new(32_000_000).unwrap(),
        115200,
        DataBits::Eight,
        Parity::None,
        StopDuration::One,
    )
    .unwrap();
    timer.configure(
        ch,
        TimerMode::Periodic,
        TimerTicks::from_count(u32::MAX).unwrap(),
    );
    timer.enable(ch);
    emit(&console, b"WS53 UART21 BEGIN phases=3 pad=17 tx=8\r\n");
    put(1, 3);
    let before = timer.current_value(ch).unwrap();
    marker.set_high();
    riscv::asm::fence();
    crg.m_clken1().modify(|_, w| w.uart_h0_enable().clear_bit());
    let prepared = old & !16;
    put(14, u32::from(crg.m_clken1().read().bits()));
    assert_eq!(snapshot()[14], u32::from(prepared));
    unsafe { interrupt::enable(irq) };
    let failure = match unsafe {
        Uart::initialize_h0(p.UART1, cfg, SignalPath::Pins, &mut Delay(&mut timer))
    } {
        Err(e) => e,
        Ok(_) => fail(10),
    };
    assert_eq!(failure.error(), H0InitError::InterruptsEnabled);
    put(24, 1);
    let (token, _) = failure.into_parts();
    unsafe { interrupt::disable(irq) };
    assert_eq!(crg.m_clken1().read().bits(), prepared);
    crg.uart_soft_reset()
        .modify(|_, w| w.uart_h0_reset_n().clear_bit());
    let failure = match unsafe {
        Uart::initialize_h0(token, cfg, SignalPath::Pins, &mut Delay(&mut timer))
    } {
        Err(e) => e,
        Ok(_) => fail(11),
    };
    assert_eq!(failure.error(), H0InitError::ResetAsserted);
    put(25, 3);
    let (token, _) = failure.into_parts();
    assert_eq!(crg.m_clken1().read().bits(), prepared);
    crg.uart_soft_reset()
        .modify(|_, w| w.uart_h0_reset_n().set_bit());
    Delay(&mut timer).delay_us(10);
    let mut uart =
        unsafe { Uart::initialize_h0(token, cfg, SignalPath::Pins, &mut Delay(&mut timer)) }
            .unwrap();
    for (i, v) in [
        u32::from(crg.m_clken1().read().bits()),
        u32::from(crg.uart_soft_reset().read().bits()),
        u32::from(interrupt::is_enabled(irq) || interrupt::is_pending(irq)),
        regs.intr_en().read().bits(),
        regs.uart_ctl().read().bits(),
        regs.modem_ctl().read().bits(),
        u32::from(regs.intr_id().read().fifo_en_s().bit()),
        u32::from(uart.tx_flushed()),
    ]
    .into_iter()
    .enumerate()
    {
        put(if i == 0 { 15 } else { 16 + i }, v)
    }
    // Keep the returned configuration intact until its first physical packet.
    packet(&uart, 2);
    drain(&uart);
    riscv::asm::fence();
    marker.set_low();
    riscv::asm::fence();
    let after = timer.current_value(ch).unwrap();
    for (j, v) in [0, before, after, before.wrapping_sub(after), 264, 0, 0, 1]
        .into_iter()
        .enumerate()
    {
        put(32 + j, v)
    }
    Delay(&mut timer).delay_ms(2);
    for phase in 1usize..3 {
        let before = timer.current_value(ch).unwrap();
        marker.set_high();
        riscv::asm::fence();
        unsafe {
            uart.reset_and_configure(
                cfg,
                if phase == 1 {
                    SignalPath::InternalLoopback
                } else {
                    SignalPath::Pins
                },
                &mut Delay(&mut timer),
            )
        }
        .unwrap();
        if phase == 1 {
            for byte in 0..=255u8 {
                uart.try_write_byte(byte).unwrap();
                drain(&uart);
                let mut got = None;
                for _ in 0..100_000 {
                    match uart.read_byte() {
                        Ok(v) => {
                            got = Some(v);
                            break;
                        }
                        Err(nb::Error::WouldBlock) => {}
                        Err(nb::Error::Other(_)) => fail(12),
                    }
                }
                assert_eq!(got, Some(byte));
            }
            assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));
        } else {
            packet(&uart, 1)
        }
        drain(&uart);
        riscv::asm::fence();
        marker.set_low();
        riscv::asm::fence();
        let after = timer.current_value(ch).unwrap();
        for (j, v) in [
            phase as u32,
            before,
            after,
            before.wrapping_sub(after),
            if phase == 1 { 256 } else { 264 },
            if phase == 1 { 256 } else { 0 },
            regs.modem_ctl().read().bits(),
            1,
        ]
        .into_iter()
        .enumerate()
        {
            put(32 + 8 * phase + j, v)
        }
        Delay(&mut timer).delay_ms(2);
    }
    // Isolate the UART pads before restoring the externally owned mux/pulls.
    unsafe { uart.reset_and_configure(cfg, SignalPath::InternalLoopback, &mut Delay(&mut timer)) }
        .unwrap();
    restore(rx);
    restore(tx);
    {
        let _pin = marker.free();
    }
    let mut restore_marker = unsafe { PadControl::claim(Pad::Mgpio17) }.unwrap();
    restore_marker.configure(original).unwrap();
    assert_eq!(restore_marker.configuration().unwrap(), original);
    assert_eq!(gpio.gpio_sw_oen().read().bits(), directions);
    put(26, 1);
    timer.disable(ch);
    timer.clear_interrupt(ch);
    interrupt::clear_pending(ch.interrupt());
    put(27, 1);
    put(28, memory.ccore_reset_status().read().bits());
    put(29, u32::from(crg.m_clken1().read().bits()));
    put(30, 256);
    put(31, 264);
    assert_eq!(snapshot()[28], 3);
    assert_eq!(crg.m_clken1().read().bits(), old);
    assert_eq!(crg.uart_soft_reset().read().bits(), reset);
    assert!(!riscv::register::mstatus::read().mie());
    put(1, 5);
    for (i, row) in snapshot().chunks(8).enumerate() {
        emit(&console, b"WS53 UART21 WORDS row=");
        hex(&console, i as u32);
        for &word in row {
            emit(&console, b" ");
            hex(&console, word)
        }
        emit(&console, b"\r\n");
    }
    emit(
        &console,
        b"WS53 UART21 PASS constructor=1 guards=2 tx=528 rx=256 restored=1\r\n",
    );
    for _ in 0..1_000_000 {
        if console.tx_flushed() {
            ws53_uart21_done()
        }
    }
    fail(33)
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    fail(0xff)
}
