//! Read/observe TX interrupt semantics before enabling an async TX API.
//! Only UART2 inherited console and its IRQ; no pad or clock configuration.
use core::cell::Cell;
use critical_section::Mutex;
use hisi_hal::{
    interrupt,
    peripherals::Uart2,
    uart::{Uart, UartPort},
};
static EVENT: Mutex<Cell<Option<[u32; 6]>>> = Mutex::new(Cell::new(None));
fn regs() -> &'static ws53_pac::uart0::RegisterBlock {
    unsafe { &*Uart2::ptr() }
}
fn snapshot() -> [u32; 6] {
    let r = regs();
    [
        u32::from(r.intr_id().read().intr_id().bits()),
        u32::from(r.intr_id().read().intr_id().bits()),
        r.intr_status().read().bits(),
        r.tx_fifo_cnt().read().bits(),
        r.line_status().read().bits(),
        r.intr_en().read().bits(),
    ]
}
fn disarm() {
    unsafe { interrupt::disable(UartPort::Uart2.interrupt()) };
    regs()
        .intr_en()
        .modify(|_, w| w.tran_em_intr_en().clear_bit());
}
#[unsafe(no_mangle)]
extern "C" fn UART_2() {
    let data = snapshot();
    disarm();
    critical_section::with(|cs| {
        assert!(EVENT.borrow(cs).replace(Some(data)).is_none());
    });
}
fn arm(programmable: bool) {
    disarm();
    regs()
        .intr_en()
        .modify(|_, w| w.tran_em_intr_en().set_bit().ptim_en().bit(programmable));
}
fn wait() -> [u32; 6] {
    unsafe {
        interrupt::enable(UartPort::Uart2.interrupt());
        riscv::interrupt::enable();
    }
    let mut budget = 100_000_000;
    loop {
        if let Some(data) = critical_section::with(|cs| EVENT.borrow(cs).take()) {
            return data;
        }
        budget -= 1;
        assert_ne!(budget, 0, "TX interrupt timeout");
        core::hint::spin_loop();
    }
}
fn flush(uart: &Uart<'_, Uart2<'_>>) {
    let mut budget = 100_000_000;
    while !uart.tx_flushed() {
        budget -= 1;
        assert_ne!(budget, 0, "TX flush timeout");
    }
}
fn report(uart: &Uart<'_, Uart2<'_>>, mode: u32, index: u32, data: [u32; 6]) {
    super::emit(uart, b"WS53 TXIRQ SAMPLE ");
    for word in [mode, index].into_iter().chain(data) {
        let mut buf = [0; 8];
        for (i, ch) in buf.iter_mut().enumerate() {
            *ch = b"0123456789abcdef"[((word >> (28 - i * 4)) & 15) as usize];
        }
        super::emit(uart, &buf);
    }
    super::emit(uart, b"\r\n");
    flush(uart);
}
pub fn check(uart: &mut Uart<'_, Uart2<'_>>) {
    riscv::interrupt::disable();
    let r = regs();
    let inherited = r.intr_en().read().bits();
    report(uart, 2, 0, snapshot());
    assert_eq!(inherited & !5, 0);
    // Vendor console RX enables are inherited; the application has explicitly
    // taken over this whole UART. Mask them during the TX-only observation.
    uart.set_receive_interrupts(false);
    unsafe { interrupt::disable(UartPort::Uart2.interrupt()) };
    assert_eq!(r.intr_en().read().bits(), 0);
    assert_eq!(r.halt_tx().read().bits(), 0);
    assert_eq!(r.rx_fifo_cnt().read().bits(), 0);
    for programmable in [false, true] {
        let mode = u32::from(programmable);
        flush(uart);
        arm(programmable);
        let polled = snapshot();
        disarm();
        report(uart, mode, 0, polled);
        // Repeated empty rearm must not require transmitting another byte.
        for index in 1..=3 {
            arm(programmable);
            let data = wait();
            riscv::interrupt::disable();
            report(uart, mode, index, data);
        }
        // Queue64 bytes with TX halted, then observe the actual IRQ on drain.
        super::emit(uart, b"WS53 TXIRQ DATA ");
        flush(uart);
        r.halt_tx().modify(|_, w| w.halt_tx().set_bit());
        for _ in 0..64 {
            assert!(uart.try_write_byte(b'U').is_ok());
        }
        assert_eq!(r.tx_fifo_cnt().read().tx_fifo_level().bits(), 64);
        arm(programmable);
        let full = snapshot();
        r.halt_tx().modify(|_, w| w.halt_tx().clear_bit());
        let drained = wait();
        riscv::interrupt::disable();
        flush(uart);
        super::emit(uart, b"\r\n");
        flush(uart);
        report(uart, mode, 4, full);
        report(uart, mode, 5, drained);
        // Mask/rearm while line may still be shifting: IIR/LSR distinguish
        // FIFO threshold from physical transmission completion.
        super::emit(uart, b"WS53 TXIRQ BYTE ");
        flush(uart);
        r.halt_tx().modify(|_, w| w.halt_tx().set_bit());
        assert!(uart.try_write_byte(b'Z').is_ok());
        arm(programmable);
        r.halt_tx().modify(|_, w| w.halt_tx().clear_bit());
        let single = wait();
        riscv::interrupt::disable();
        flush(uart);
        super::emit(uart, b"\r\n");
        flush(uart);
        report(uart, mode, 6, single);
    }
    disarm();
    regs().intr_en().modify(|_, w| w.ptim_en().clear_bit());
    r.intr_en().modify(|_, w| {
        w.rece_line_stat_intr_en()
            .bit(inherited & 1 != 0)
            .rece_data_intr_en()
            .bit(inherited & 4 != 0)
    });
    assert_eq!(r.intr_en().read().bits(), inherited);
    super::emit(
        uart,
        b"WS53 TXIRQ OBSERVED modes=2 samples=14 rearm=6 restored=1\r\n",
    );
}
