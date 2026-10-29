//! QEMU-only receive IRQ scenario. No peripheral data is read in the ISR.
//! Requires an explicitly injected QEMU character-timeout delay (not silicon timing).
use core::cell::Cell;
use critical_section::Mutex;
use hisi_hal::interrupt;
use hisi_hal::peripherals::{Uart0, Uart1, Uart2};
use hisi_hal::uart::{ReceiveInterrupt, Uart, UartInstance};

static EVENTS: Mutex<[Cell<Option<ReceiveInterrupt>>; 3]> =
    Mutex::new([const { Cell::new(None) }; 3]);
static COUNTS: Mutex<[Cell<u32>; 3]> = Mutex::new([const { Cell::new(0) }; 3]);

fn record<T: UartInstance>() {
    // Only PAC/controller state is touched before this short metadata update.
    // No shared &mut Uart, receive operation, poll, or user callback in the ISR.
    let event = unsafe { Uart::<T>::on_receive_interrupt() };
    critical_section::with(|cs| {
        EVENTS.borrow(cs)[T::PORT.index()].set(Some(event));
        let count = &COUNTS.borrow(cs)[T::PORT.index()];
        count.set(count.get() + 1);
    });
}

#[unsafe(no_mangle)]
extern "C" fn UART_0() {
    record::<Uart0<'static>>();
}
#[unsafe(no_mangle)]
extern "C" fn UART_1() {
    record::<Uart1<'static>>();
}
#[unsafe(no_mangle)]
extern "C" fn UART_2() {
    record::<Uart2<'static>>();
}

fn count<T: UartInstance>() -> u32 {
    critical_section::with(|cs| COUNTS.borrow(cs)[T::PORT.index()].get())
}

fn arm<T: UartInstance>(uart: &mut Uart<'_, T>) {
    assert!(!interrupt::is_enabled(T::PORT.interrupt()));
    critical_section::with(|cs| EVENTS.borrow(cs)[T::PORT.index()].set(None));
    uart.set_receive_interrupts(true);
    unsafe { interrupt::enable(T::PORT.interrupt()) };
}

fn expect<T: UartInstance>(expected: ReceiveInterrupt, expected_count: u32) {
    unsafe { riscv::interrupt::enable() };
    let mut remaining = 10_000_000;
    loop {
        let event = critical_section::with(|cs| EVENTS.borrow(cs)[T::PORT.index()].get());
        if let Some(event) = event {
            riscv::interrupt::disable();
            assert_eq!(event, expected);
            assert_eq!(count::<T>(), expected_count);
            assert!(!interrupt::is_enabled(T::PORT.interrupt()));
            assert!(!interrupt::is_pending(T::PORT.interrupt()));
            return;
        }
        remaining -= 1;
        assert_ne!(remaining, 0, "missing UART interrupt");
        core::hint::spin_loop();
    }
}

fn no_event<T: UartInstance>(expected_count: u32) {
    unsafe { riscv::interrupt::enable() };
    for _ in 0..10_000 {
        core::hint::spin_loop();
    }
    riscv::interrupt::disable();
    assert_eq!(
        count::<T>(),
        expected_count,
        "masked or drained FIFO retriggered"
    );
}

pub fn check<T: UartInstance>(uart: &mut Uart<'_, T>) {
    let irq = T::PORT.interrupt();
    riscv::interrupt::disable();
    unsafe { interrupt::disable(irq) };
    let regs = unsafe { uart.register_block() };
    unsafe {
        regs.intr_en().write_with_zero(|w| w);
        regs.fifo_ctl()
            .write_with_zero(|w| w.fifo_en().set_bit().rx_fifo_rst().set_bit());
    }
    regs.far().modify(|_, w| w.far().set_bit());
    let push = |byte: u8, parity: bool, framing: bool| unsafe {
        regs.rx_fifo_write().write_with_zero(|w| {
            w.rx_fifo_write()
                .bits(byte)
                .rx_fifo_pe()
                .bit(parity)
                .rx_fifo_fe()
                .bit(framing)
        });
    };

    // Queued data while the peripheral is masked must survive rearming. Enable
    // the controller first to prove that the peripheral mask itself suppresses it.
    unsafe { interrupt::enable(irq) };
    push(0x11, false, false);
    no_event::<T>(0);
    unsafe { interrupt::disable(irq) };
    arm(uart);
    expect::<T>(ReceiveInterrupt::DataAvailable, 1);
    assert_eq!(regs.rx_fifo_cnt().read().rx_fifo_level().bits(), 1);
    assert!(!regs.intr_en().read().rece_data_intr_en().bit());
    assert!(!regs.intr_en().read().rece_line_stat_intr_en().bit());
    assert_eq!(uart.read_byte(), Ok(0x11));

    // Data that arrives after the one-shot ISR is preserved until explicit rearm.
    push(0x22, false, false);
    no_event::<T>(1);
    arm(uart);
    expect::<T>(ReceiveInterrupt::DataAvailable, 2);
    assert_eq!(uart.read_byte(), Ok(0x22));

    // Error IRQ deferral must not consume the read-clear flags or a good prefix.
    push(0x33, false, false);
    push(0x44, true, true);
    push(0x55, false, false);
    arm(uart);
    expect::<T>(ReceiveInterrupt::LineStatus, 3);
    assert_eq!(regs.rx_fifo_cnt().read().rx_fifo_level().bits(), 3);
    assert_eq!(uart.read_byte(), Ok(0x33));
    let Err(nb::Error::Other(error)) = uart.read_byte() else {
        panic!()
    };
    assert!(error.framing() && error.parity() && !error.overrun());
    assert_eq!(uart.read_byte(), Ok(0x55));

    // While masked, an overrun still reports the lost byte and preserves all 64
    // queued bytes. It must not be accidentally acknowledged by the ISR.
    for byte in 0..64 {
        push(byte, false, false);
    }
    push(0xff, false, false);
    arm(uart);
    expect::<T>(ReceiveInterrupt::LineStatus, 4);
    let Err(nb::Error::Other(error)) = uart.read_byte() else {
        panic!()
    };
    assert!(error.overrun());
    for byte in 0..64 {
        assert_eq!(uart.read_byte(), Ok(byte));
    }

    // Partial FIFO below the 16-byte threshold uses the injected QEMU timeout.
    unsafe {
        regs.fifo_ctl()
            .write_with_zero(|w| w.fifo_en().set_bit().rx_empty_trig().quarter());
    }
    push(0x66, false, false);
    arm(uart);
    expect::<T>(ReceiveInterrupt::CharacterTimeout, 5);
    assert_eq!(uart.read_byte(), Ok(0x66));

    // Cancellation suppresses delivery without throwing away data. Rearm must
    // still deliver it; afterwards an empty FIFO must not create notifications.
    unsafe {
        regs.fifo_ctl().write_with_zero(|w| w.fifo_en().set_bit());
    }
    arm(uart);
    push(0x77, false, false);
    assert!(interrupt::is_pending(irq));
    uart.set_receive_interrupts(false);
    // Cancelling an already latched notification requires disabling/clearing
    // the controller as well as masking the peripheral. No data is consumed.
    unsafe { interrupt::disable(irq) };
    no_event::<T>(5);
    arm(uart);
    expect::<T>(ReceiveInterrupt::DataAvailable, 6);
    assert_eq!(uart.read_byte(), Ok(0x77));
    assert!(matches!(uart.read_byte(), Err(nb::Error::WouldBlock)));
    arm(uart);
    no_event::<T>(6);
    uart.set_receive_interrupts(false);
    unsafe { interrupt::disable(irq) };
    regs.far().modify(|_, w| w.far().clear_bit());
}
