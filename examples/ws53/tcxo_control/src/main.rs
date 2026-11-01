//! QEMU-only TCXO fixture. No real counter/epoch/clock handoff has been established.
#![no_std]
#![no_main]
use core::num::NonZeroU16;
#[cfg(not(any(feature = "timeout-smoke", feature = "transaction-smoke")))]
use core::num::NonZeroU32;
use hisi_hal::Peripherals;
use hisi_hal::peripherals::Uart2;
use hisi_hal::tcxo::{TcxoDriver, TcxoError};
use hisi_hal::uart::Uart;
use hisi_riscv_rt::entry;
fn emit(uart: &Uart<'_, Uart2<'_>>, bytes: &[u8]) {
    for &byte in bytes {
        let mut budget = 1_000_000;
        while uart.try_write_byte(byte).is_err() {
            budget -= 1;
            assert!(budget != 0, "UART stopped");
        }
    }
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    // Only the named QEMU fixture establishes these ownership assumptions.
    let uart = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    #[cfg(feature = "transaction-smoke")]
    let mut uart = uart;
    let mut tcxo = TcxoDriver::new(
        unsafe { p.TCXO.assume_application_ownership() },
        NonZeroU16::new(256).unwrap(),
    );
    emit(&uart, b"WS53 TCXO QEMU begin\n");
    assert_eq!(tcxo.read_counter(), Err(TcxoError::Disabled));
    assert_eq!(tcxo.clear_counter(), Err(TcxoError::Disabled));
    tcxo.enable().unwrap();
    #[cfg(feature = "transaction-smoke")]
    transaction(&mut tcxo, &mut uart);
    #[cfg(all(feature = "timeout-smoke", not(feature = "transaction-smoke")))]
    {
        assert_eq!(tcxo.read_counter(), Err(TcxoError::SnapshotTimeout));
        emit(&uart, b"WS53 PASS TCXO timeout distinct from zero\n");
    }
    #[cfg(not(any(feature = "timeout-smoke", feature = "transaction-smoke")))]
    {
        let frequency = NonZeroU32::new(32_000_000).unwrap();
        let first = tcxo.read_counter().unwrap();
        assert!(first.ticks() >= 0x1234_0000_0000); // constructor/enable must preserve fixture epoch
        let mut advanced = false;
        for _ in 0..1000 {
            let next = tcxo.read_counter().unwrap();
            if next.ticks() > first.ticks() {
                assert!(
                    !next
                        .checked_duration_since(first, frequency)
                        .unwrap()
                        .is_zero()
                );
                advanced = true;
                break;
            }
        }
        assert!(advanced);
        tcxo.clear_counter().unwrap();
        let cleared = tcxo.read_counter().unwrap();
        assert!(cleared.ticks() < first.ticks());
        assert!(cleared.checked_duration_since(first, frequency).is_none());
        // With sticky CLEAR readback, refresh must not repeatedly clear the counter.
        let mut advanced = false;
        for _ in 0..1000 {
            let next = tcxo.read_counter().unwrap();
            if next.ticks() > cleared.ticks() {
                advanced = true;
                break;
            }
        }
        assert!(advanced);
        emit(&uart, b"WS53 PASS TCXO epoch ticks clear\n");
    }
    emit(&uart, b"WS53 TCXO QEMU done\n");
    loop {
        riscv::asm::wfi();
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        riscv::asm::wfi();
    }
}

#[cfg(feature = "transaction-smoke")]
fn transaction(tcxo: &mut TcxoDriver<'_>, uart: &mut Uart<'_, Uart2<'_>>) {
    fn stage(uart: &mut Uart<'_, Uart2<'_>>, message: &[u8]) {
        emit(uart, message);
        let mut budget = 100_000_000;
        loop {
            match uart.read_byte() {
                Ok(b'g') => break,
                Err(nb::Error::WouldBlock) => (),
                _ => panic!("bad host response"),
            }
            budget -= 1;
            assert_ne!(budget, 0, "host did not change fault");
        }
    }
    {
        let mut snapshot = tcxo.begin_snapshot().unwrap();
        assert_eq!(snapshot.try_read(), Err(nb::Error::WouldBlock));
    }
    assert!(matches!(
        tcxo.begin_snapshot(),
        Err(TcxoError::SnapshotBusy)
    ));
    assert_eq!(tcxo.read_counter(), Err(TcxoError::SnapshotBusy));
    assert_eq!(tcxo.enable(), Err(TcxoError::SnapshotBusy));
    assert_eq!(tcxo.clear_counter(), Err(TcxoError::SnapshotBusy));
    assert_eq!(tcxo.try_discard_snapshot(), Err(nb::Error::WouldBlock));
    stage(uart, b"WS53 TCXO recover1\n");
    tcxo.try_discard_snapshot().unwrap();
    let first = tcxo.read_counter().unwrap();
    assert!(first.ticks() >= 0x1234_0000_0000);
    stage(uart, b"WS53 TCXO stall2\n");
    assert_eq!(tcxo.read_counter(), Err(TcxoError::SnapshotTimeout));
    assert_eq!(tcxo.read_counter(), Err(TcxoError::SnapshotBusy));
    assert!(matches!(
        tcxo.begin_snapshot(),
        Err(TcxoError::SnapshotBusy)
    ));
    assert_eq!(tcxo.clear_counter(), Err(TcxoError::SnapshotBusy));
    stage(uart, b"WS53 TCXO recover2\n");
    // Automatic discard on next begin: old late result is never returned.
    let second = tcxo.read_counter().unwrap();
    assert!(second.ticks() >= first.ticks());
    let _ = tcxo.begin_snapshot().unwrap();
    assert_eq!(tcxo.enable(), Err(TcxoError::SnapshotBusy));
    let third = tcxo.read_counter().unwrap();
    assert!(third.ticks() >= second.ticks());
    {
        let mut snapshot = tcxo.begin_snapshot().unwrap();
        let value = snapshot.try_read().unwrap();
        assert_eq!(snapshot.try_read(), Ok(value));
        assert!(value.ticks() >= third.ticks());
    }
    tcxo.try_discard_snapshot().unwrap();
    emit(uart, b"WS53 PASS TCXO cancel timeout recover cached\n");
}
