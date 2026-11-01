//! Physical/QEMU UART2 receive notifications with external host input.
//! No synthetic FIFO/error MMIO and no access to UART0/1 or pinmux.
use core::cell::Cell;
use critical_section::Mutex;
use hisi_hal::interrupt;
use hisi_hal::peripherals::Uart2;
use hisi_hal::uart::{ReceiveInterrupt, Uart, UartPort};

static EVENT: Mutex<Cell<Option<ReceiveInterrupt>>> = Mutex::new(Cell::new(None));
static COUNT: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));

#[unsafe(no_mangle)]
extern "C" fn UART_2() {
    // No driver reference is shared with main; this masks both notification
    // levels and leaves all RX bytes/error status to the owning main context.
    let event = unsafe { Uart::<Uart2<'static>>::on_receive_interrupt() };
    critical_section::with(|cs| {
        EVENT.borrow(cs).set(Some(event));
        COUNT.borrow(cs).set(COUNT.borrow(cs).get() + 1);
    });
}

fn count() -> u32 {
    critical_section::with(|cs| COUNT.borrow(cs).get())
}

fn read_expected(uart: &mut Uart<'_, Uart2<'_>>, expected: &[u8]) {
    for &byte in expected {
        let mut budget = 10_000_000;
        loop {
            match uart.read_byte() {
                Ok(actual) => {
                    assert_eq!(actual, byte);
                    break;
                }
                Err(nb::Error::WouldBlock) => {}
                Err(nb::Error::Other(_)) => panic!("UART2 receive error"),
            }
            budget -= 1;
            assert_ne!(budget, 0, "UART2 input timeout");
            core::hint::spin_loop();
        }
    }
    assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));
}

fn no_notification(expected: u32) {
    unsafe { riscv::interrupt::enable() };
    for _ in 0..10_000 {
        core::hint::spin_loop();
    }
    riscv::interrupt::disable();
    assert_eq!(count(), expected);
}

pub fn check(uart: &mut Uart<'_, Uart2<'_>>) {
    let irq = UartPort::Uart2.interrupt();
    riscv::interrupt::disable();
    unsafe { interrupt::disable(irq) };
    uart.set_receive_interrupts(false);
    assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));
    for (stage, (marker, bytes)) in [
        (&b"WS53 UART2 IRQ ARM 0\r\n"[..], &b"\x00\xff\x51"[..]),
        (&b"WS53 UART2 IRQ ARM 1\r\n"[..], &b"\xa5\x5a\x53"[..]),
    ]
    .into_iter()
    .enumerate()
    {
        critical_section::with(|cs| EVENT.borrow(cs).set(None));
        uart.set_receive_interrupts(true);
        unsafe {
            interrupt::enable(irq);
            riscv::interrupt::enable();
        }
        super::emit(uart, marker);
        let mut budget = 100_000_000;
        let event = loop {
            if let Some(event) = critical_section::with(|cs| EVENT.borrow(cs).get()) {
                break event;
            }
            budget -= 1;
            assert_ne!(budget, 0, "UART2 IRQ timeout");
            core::hint::spin_loop();
        };
        riscv::interrupt::disable();
        // Inherited FIFO thresholds can differ; this does not equate a data
        // threshold interrupt with the silicon character-timeout duration.
        assert!(matches!(
            event,
            ReceiveInterrupt::DataAvailable | ReceiveInterrupt::CharacterTimeout
        ));
        assert_eq!(count(), stage as u32 + 1);
        assert!(!interrupt::is_enabled(irq));
        assert!(!interrupt::is_pending(irq));
        read_expected(uart, bytes);
        no_notification(stage as u32 + 1);
    }
    // Cancel both levels before the host sends a byte. It must remain readable
    // without producing another handler notification.
    uart.set_receive_interrupts(true);
    unsafe { interrupt::enable(irq) };
    uart.set_receive_interrupts(false);
    unsafe { interrupt::disable(irq) };
    unsafe { riscv::interrupt::enable() };
    super::emit(uart, b"WS53 UART2 IRQ CANCEL\r\n");
    read_expected(uart, b"\x3c");
    no_notification(2);
    assert!(!interrupt::is_enabled(irq));
    super::emit(uart, b"WS53 PASS uart2_irq=42 notifications=2 cancel=1\r\n");
}
