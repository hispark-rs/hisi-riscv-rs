//! HAL/runtime integration test. UART2 pads and its clock must already be set
//! by the vendor boot environment. This is not yet the general UART HAL demo.
#![no_std]
#![no_main]

use core::cell::Cell;
use critical_section::Mutex;
use hisi_hal::peripherals::Uart2;
use hisi_hal::timer::{TimerChannel, TimerDriver, TimerMode, TimerTicks};
use hisi_hal::uart::Uart;
use hisi_hal::{Peripherals, interrupt};
use hisi_riscv_rt::entry;

#[cfg(feature = "i2c-wire-probe")]
mod i2c_wire;
#[cfg(feature = "spi-wire-probe")]
mod spi_wire;
#[cfg(all(feature = "spi-wire-probe", feature = "i2c-wire-probe"))]
compile_error!("choose one bus wire fixture");

#[cfg(feature = "uart-line-smoke")]
mod uart_line;
#[cfg(all(
    feature = "uart-line-smoke",
    any(
        feature = "uart-rx-smoke",
        feature = "uart-error-smoke",
        feature = "uart-irq-smoke",
        feature = "uart2-irq-smoke",
        feature = "gpio-smoke",
        feature = "gpio17-smoke",
        feature = "timer1-gpio17-smoke",
        feature = "tcxo-snapshot-smoke",
        feature = "rtc-smoke",
        feature = "wdt-smoke",
        feature = "trng-smoke",
        feature = "tsensor-smoke",
        feature = "mdma-smoke",
        feature = "pwm0-smoke"
    )
))]
compile_error!("choose the isolated UART line fixture");

#[cfg(feature = "gpio-smoke")]
mod gpio;
#[cfg(feature = "gpio17-smoke")]
mod gpio17;
#[cfg(all(feature = "pwm0-smoke", not(feature = "pwm0-finite-probe")))]
mod pwm0;
#[cfg(feature = "pwm0-finite-probe")]
mod pwm_finite;
#[cfg(all(feature = "pwm0-finite-probe", feature = "pwm0-clock-probe"))]
compile_error!("choose one PWM fixture");
#[cfg(all(feature = "wdt-smoke", not(feature = "wdt-handshake-probe")))]
mod wdt_probe;
#[cfg(feature = "wdt-handshake-probe")]
#[path = "wdt_handshake.rs"]
mod wdt_probe;
#[cfg(all(feature = "wdt-handshake-probe", feature = "wdt-settle-probe"))]
compile_error!("choose one WDT diagnostic");
#[cfg(all(
    feature = "wdt-smoke",
    any(
        feature = "trng-smoke",
        feature = "tsensor-smoke",
        feature = "mdma-smoke",
        feature = "rtc-smoke",
        feature = "tcxo-snapshot-smoke",
        feature = "timer1-gpio17-smoke",
        feature = "pwm0-smoke"
    )
))]
compile_error!("choose one WDT/TIMER1 ownership fixture");
#[cfg(feature = "ipc-random-smoke")]
mod ipc_random_probe;
#[cfg(feature = "secure-boot-read-probe")]
mod secure_boot_read_probe;
#[cfg(feature = "sfc-read-probe")]
mod sfc_read_probe;
#[cfg(feature = "trng-smoke")]
mod trng_probe;
#[cfg(all(
    feature = "trng-smoke",
    any(
        feature = "tsensor-smoke",
        feature = "mdma-smoke",
        feature = "rtc-smoke",
        feature = "tcxo-snapshot-smoke",
        feature = "timer1-gpio17-smoke",
        feature = "pwm0-smoke"
    )
))]
compile_error!("choose one TRNG/TIMER1 ownership fixture");
#[cfg(feature = "tsensor-smoke")]
mod tsensor_probe;
#[cfg(all(
    feature = "tsensor-smoke",
    any(
        feature = "mdma-smoke",
        feature = "rtc-smoke",
        feature = "tcxo-snapshot-smoke",
        feature = "timer1-gpio17-smoke",
        feature = "pwm0-smoke"
    )
))]
compile_error!("choose one temperature/TIMER1 ownership fixture");
#[cfg(feature = "mdma-smoke")]
mod mdma_probe;
#[cfg(feature = "rtc-smoke")]
mod rtc_probe;
#[cfg(all(
    feature = "mdma-smoke",
    any(
        feature = "rtc-smoke",
        feature = "tcxo-snapshot-smoke",
        feature = "timer1-gpio17-smoke"
    )
))]
compile_error!("choose one TIMER1 DMA fixture");
#[cfg(feature = "tcxo-snapshot-smoke")]
mod tcxo_snapshot;
#[cfg(all(
    feature = "rtc-smoke",
    any(feature = "tcxo-snapshot-smoke", feature = "timer1-gpio17-smoke")
))]
compile_error!("choose one TIMER1 ownership fixture");
#[cfg(all(feature = "pwm0-smoke", feature = "tcxo-snapshot-smoke"))]
compile_error!("choose one TCXO ownership fixture");
#[cfg(feature = "timer1-gpio17-smoke")]
mod timer1_gpio17;
#[cfg(all(feature = "tcxo-snapshot-smoke", feature = "timer1-gpio17-smoke"))]
compile_error!("choose one TIMER1 interrupt fixture");
#[cfg(all(
    feature = "timer1-gpio17-smoke",
    any(feature = "gpio-smoke", feature = "gpio17-smoke")
))]
compile_error!("choose one GPIO pad ownership fixture");
#[cfg(all(feature = "gpio17-smoke", feature = "gpio-smoke"))]
compile_error!("choose the single-pad fixture or the QEMU all-bank fixture");
#[cfg(feature = "gpio-async-smoke")]
mod gpio_async;
#[cfg(feature = "timer-async-smoke")]
mod timer_async;
#[cfg(all(
    feature = "gpio-async-smoke",
    any(
        feature = "gpio-smoke",
        feature = "gpio17-smoke",
        feature = "timer1-gpio17-smoke",
        feature = "uart-pad-probe"
    )
))]
compile_error!("choose one GPIO17/bank owner");
#[cfg(all(
    feature = "timer-async-smoke",
    any(feature = "timer1-gpio17-smoke", feature = "tcxo-snapshot-smoke")
))]
compile_error!("choose one TIMER1/TCXO fixture");
#[cfg(feature = "uart2-async-smoke")]
mod uart2_async;
#[cfg(all(
    feature = "uart2-async-smoke",
    any(feature = "uart2-irq-smoke", feature = "uart-irq-smoke")
))]
compile_error!("choose one UART2 interrupt owner");
#[cfg(feature = "uart2-irq-smoke")]
mod uart2_irq;
#[cfg(feature = "uart-error-smoke")]
mod uart_errors;
#[cfg(feature = "uart-irq-smoke")]
mod uart_irq;
#[cfg(all(feature = "uart2-irq-smoke", feature = "uart-irq-smoke"))]
compile_error!("choose the physical UART2 fixture or the QEMU all-UART fault fixture");

#[cfg(feature = "embassy-smoke")]
mod embassy_time_test;
#[cfg(all(
    feature = "embassy-smoke",
    any(
        feature = "timer-async-smoke",
        feature = "timer1-gpio17-smoke",
        feature = "tcxo-snapshot-smoke",
        feature = "pwm0-smoke",
        feature = "rtc-smoke",
        feature = "wdt-smoke",
        feature = "trng-smoke",
        feature = "tsensor-smoke",
        feature = "mdma-smoke",
        feature = "uart-line-smoke"
    )
))]
compile_error!("Embassy owns TIMER and TCXO for the whole process");

#[cfg(feature = "uart-tx-irq-probe")]
mod uart_tx_irq_probe;
#[cfg(all(
    feature = "uart-tx-irq-probe",
    any(
        feature = "uart2-async-smoke",
        feature = "uart2-irq-smoke",
        feature = "uart-irq-smoke",
        feature = "uart-line-smoke"
    )
))]
compile_error!("choose one UART interrupt fixture");
#[cfg(feature = "uart2-async-tx-smoke")]
mod uart2_async_tx;
#[cfg(all(
    feature = "uart2-async-tx-smoke",
    any(
        feature = "uart-tx-irq-probe",
        feature = "uart2-async-smoke",
        feature = "uart2-irq-smoke",
        feature = "uart-irq-smoke",
        feature = "uart-line-smoke"
    )
))]
compile_error!("choose one UART interrupt fixture");
static TICKS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
// Exercise initialized RAM data as well as zeroed BSS.
static mut DATA_PROBE: u32 = 0x53ac_0123;

fn emit(uart: &Uart<'_, Uart2<'_>>, bytes: &[u8]) {
    for &byte in bytes {
        let mut remaining = 1_000_000;
        while uart.try_write_byte(byte).is_err() {
            remaining -= 1;
            if remaining == 0 {
                fail();
            }
            core::hint::spin_loop();
        }
    }
}

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    // This RAM takeover test requires external confirmation that the old acore
    // UART2/TIMER users have stopped and the control core does not use them.
    // The runner must establish this before loading, not infer it from `take`.
    #[allow(unused_mut)]
    let mut uart = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    let mut timer = TimerDriver::new(unsafe { p.TIMER.assume_application_ownership() });
    let channel = TimerChannel::Channel0;
    if unsafe { core::ptr::addr_of!(DATA_PROBE).read_volatile() } != 0x53ac_0123
        || critical_section::with(|cs| TICKS.borrow(cs).get()) != 0
    {
        fail();
    }
    emit(&uart, b"WS53 RAM data/bss OK\r\n");
    timer.configure(
        channel,
        TimerMode::Periodic,
        TimerTicks::from_micros(10_000).unwrap(),
    );
    interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
    unsafe {
        interrupt::enable(channel.interrupt());
    }
    timer.enable(channel);
    unsafe {
        riscv::interrupt::enable();
    }
    // A finite poll budget makes a missing IRQ visible instead of hanging a HIL
    // run. ISR records/acks only; serial traffic stays in normal context.
    let mut budget = 100_000_000u32;
    while critical_section::with(|cs| TICKS.borrow(cs).get()) < 3 {
        budget -= 1;
        if budget == 0 {
            emit(&uart, b"WS53 FAIL IRQ timeout\r\n");
            fail();
        }
        core::hint::spin_loop();
    }
    riscv::interrupt::disable();
    timer.disable(channel);
    timer.clear_interrupt(channel);
    unsafe {
        interrupt::disable(channel.interrupt());
    }
    emit(&uart, b"WS53 PASS timer_irq=53 count=3\r\n");
    #[cfg(feature = "spi-wire-probe")]
    spi_wire::check(p.SPI0, p.GPIO0, p.GPIO1, &mut timer, &uart);
    #[cfg(feature = "i2c-wire-probe")]
    i2c_wire::check(p.I2C0, p.I2C1, p.GPIO0, &mut timer, &uart);
    #[cfg(feature = "secure-boot-read-probe")]
    secure_boot_read_probe::check(&uart, p.EFUSE);
    #[cfg(feature = "sfc-read-probe")]
    sfc_read_probe::check(&uart);
    #[cfg(feature = "uart-rx-smoke")]
    {
        for &expected in b"\x00\xffWS53-RX\n" {
            let mut remaining = 10_000_000u32;
            loop {
                match uart.read_byte() {
                    Ok(byte) => {
                        if byte != expected {
                            emit(&uart, b"WS53 FAIL UART RX data\r\n");
                            fail();
                        }
                        break;
                    }
                    Err(nb::Error::Other(_)) => {
                        emit(&uart, b"WS53 FAIL UART RX error\r\n");
                        fail();
                    }
                    Err(nb::Error::WouldBlock) => {}
                }
                remaining -= 1;
                if remaining == 0 {
                    emit(&uart, b"WS53 FAIL UART RX timeout\r\n");
                    fail();
                }
                core::hint::spin_loop();
            }
        }
        if !matches!(uart.read_byte(), Err(nb::Error::WouldBlock)) {
            emit(&uart, b"WS53 FAIL UART RX empty\r\n");
            fail();
        }
        emit(&uart, b"WS53 PASS uart2_rx=10 empty=1\r\n");
    }
    #[cfg(any(feature = "uart-error-smoke", feature = "uart-irq-smoke"))]
    {
        // QEMU-only fault-injection scenario. This claims all three UARTs and
        // resets their receive FIFOs; it is not the default RAM/HIL image.
        let mut uart0 = unsafe { Uart::from_configured(p.UART0.assume_application_ownership()) };
        let mut uart1 = unsafe { Uart::from_configured(p.UART1.assume_application_ownership()) };
        #[cfg(feature = "uart-error-smoke")]
        {
            uart_errors::check(&mut uart0);
            uart_errors::check(&mut uart1);
            uart_errors::check(&mut uart);
            emit(&uart, b"WS53 PASS uart_errors=3 partial=1 recovery=1\r\n");
        }
        #[cfg(feature = "uart-irq-smoke")]
        {
            uart_irq::check(&mut uart0);
            uart_irq::check(&mut uart1);
            uart_irq::check(&mut uart);
            emit(
                &uart,
                b"WS53 PASS uart_irq=39,41,42 notifications=6 masks=1 recovery=1\r\n",
            );
        }
    }
    #[cfg(feature = "gpio-smoke")]
    {
        // QEMU-only: the runner supplies digital pad inputs. No claim that
        // these pads are safe/available or correctly muxed on the real board.
        use hisi_hal::gpio::{GpioDriver, Pad};
        gpio::check_pads(&mut uart);
        let mut gpio0 = GpioDriver::new_gpio0(unsafe { p.GPIO0.assume_application_ownership() });
        let mut gpio1 = GpioDriver::new_gpio1(unsafe { p.GPIO1.assume_application_ownership() })
            .unwrap_or_else(|_| panic!("GPIO1 clock handoff failed"));
        let mut gpio2 = GpioDriver::new_gpio2(unsafe { p.GPIO2.assume_application_ownership() });
        gpio::check(&mut gpio0, Pad::Mgpio2, &mut uart, b'0');
        gpio::check(&mut gpio1, Pad::Agpio3, &mut uart, b'1');
        gpio::check(&mut gpio2, Pad::Sgpio2, &mut uart, b'2');
        emit(
            &uart,
            b"WS53 PASS gpio_banks=3 irq=34,33,29 notifications=6 cancel=1\r\n",
        );
    }
    #[cfg(feature = "uart2-irq-smoke")]
    uart2_irq::check(&mut uart);
    #[cfg(feature = "uart2-async-tx-smoke")]
    uart2_async_tx::check(&mut uart);
    #[cfg(feature = "uart-tx-irq-probe")]
    uart_tx_irq_probe::check(&mut uart);
    #[cfg(feature = "uart2-async-smoke")]
    uart2_async::check(&mut uart);
    #[cfg(feature = "timer-async-smoke")]
    {
        let mut tcxo = hisi_hal::tcxo::TcxoDriver::new(
            unsafe { p.TCXO.assume_application_ownership() },
            core::num::NonZeroU16::new(0xffff).unwrap(),
        );
        timer_async::check(&mut timer, &mut tcxo, &uart);
    }
    #[cfg(feature = "embassy-smoke")]
    {
        let tcxo = hisi_hal::tcxo::TcxoDriver::new(
            unsafe { p.TCXO.assume_application_ownership() },
            core::num::NonZeroU16::new(0xffff).unwrap(),
        );
        embassy_time_test::check(timer, tcxo, &mut uart);
    }
    #[cfg(feature = "gpio-async-smoke")]
    {
        let mut gpio = hisi_hal::gpio::GpioDriver::new_gpio0(unsafe {
            p.GPIO0.assume_application_ownership()
        });
        gpio_async::check(&mut gpio, &uart);
    }
    #[cfg(all(feature = "uart-line-smoke", not(feature = "uart-all-lines-smoke")))]
    uart_line::check(&mut uart, &mut timer);
    #[cfg(feature = "uart-all-lines-smoke")]
    unsafe {
        uart_line::check_all(
            &mut uart,
            &mut timer,
            p.UART0.assume_application_ownership(),
            #[cfg(not(feature = "uart-h0-init-smoke"))]
            p.UART1.assume_application_ownership(),
            #[cfg(feature = "uart-h0-init-smoke")]
            p.UART1,
            #[cfg(feature = "uart-clock-probe")]
            p.GPIO0.assume_application_ownership(),
        );
    }
    #[cfg(feature = "gpio17-smoke")]
    {
        let mut gpio = hisi_hal::gpio::GpioDriver::new_gpio0(unsafe {
            p.GPIO0.assume_application_ownership()
        });
        gpio17::check(&mut gpio, &mut uart);
    }
    #[cfg(feature = "timer1-gpio17-smoke")]
    {
        let mut gpio = hisi_hal::gpio::GpioDriver::new_gpio0(unsafe {
            p.GPIO0.assume_application_ownership()
        });
        timer1_gpio17::check(&mut gpio, &mut timer, &mut uart);
    }
    #[cfg(feature = "tcxo-snapshot-smoke")]
    {
        let mut tcxo = hisi_hal::tcxo::TcxoDriver::new(
            unsafe { p.TCXO.assume_application_ownership() },
            core::num::NonZeroU16::new(1024).unwrap(),
        );
        tcxo_snapshot::check(&mut tcxo, &mut timer, &uart);
        #[cfg(feature = "tcxo-nonblocking-smoke")]
        tcxo_snapshot::check_nonblocking(&mut tcxo, &uart);
    }
    #[cfg(feature = "rtc-smoke")]
    rtc_probe::check(
        unsafe { p.RTC0.assume_application_ownership() },
        unsafe { p.RTC1.assume_application_ownership() },
        &mut timer,
        &uart,
    );
    #[cfg(feature = "pwm0-smoke")]
    {
        let tcxo = hisi_hal::tcxo::TcxoDriver::new(
            unsafe { p.TCXO.assume_application_ownership() },
            core::num::NonZeroU16::new(1024).unwrap(),
        );
        #[cfg(feature = "pwm0-finite-probe")]
        use pwm_finite as pwm_fixture;
        #[cfg(not(feature = "pwm0-finite-probe"))]
        use pwm0 as pwm_fixture;
        pwm_fixture::check(
            unsafe { p.PWM.assume_application_ownership() },
            tcxo,
            &mut uart,
        );
    }
    #[cfg(feature = "wdt-smoke")]
    wdt_probe::check(
        unsafe { p.WDT.assume_application_ownership() },
        &mut timer,
        &uart,
    );
    #[cfg(feature = "trng-smoke")]
    trng_probe::check(
        unsafe { p.TRNG.assume_application_ownership() },
        &mut timer,
        &mut uart,
    );
    #[cfg(feature = "tsensor-smoke")]
    tsensor_probe::check(
        unsafe { p.TSENSOR.assume_application_ownership() },
        &mut timer,
        &uart,
    );
    #[cfg(feature = "mdma-smoke")]
    mdma_probe::check(
        unsafe { p.MDMA.assume_application_ownership() },
        &mut timer,
        &uart,
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
    // Acknowledge the owned timer channel, then record Rust-owned metadata.
    unsafe {
        TimerDriver::acknowledge_interrupt(TimerChannel::Channel0);
    }
    critical_section::with(|cs| {
        let ticks = TICKS.borrow(cs);
        ticks.set(ticks.get() + 1);
    });
}

fn fail() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    #[cfg(feature = "timer-async-smoke")]
    timer_async::capture_panic(_info);
    #[cfg(feature = "embassy-smoke")]
    embassy_time_test::capture_panic(_info);
    fail()
}
