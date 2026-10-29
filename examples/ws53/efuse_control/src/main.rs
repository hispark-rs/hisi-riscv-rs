//! QEMU-only immutable eFuse image. Never load on a physical board.
#![no_std]
#![no_main]
use hisi_hal::{
    Peripherals,
    efuse::{EfuseByteAddress, EfuseDriver, EfuseError},
    peripherals::Uart2,
    uart::Uart,
};
use hisi_riscv_rt::entry;
#[unsafe(no_mangle)]
static mut WS53_EFUSE_OUTPUT: [u8; 258] = [0xa5; 258];
fn emit(u: &Uart<'_, Uart2<'_>>, s: &[u8]) {
    for &b in s {
        let mut n = 1_000_000;
        while u.try_write_byte(b).is_err() {
            n -= 1;
            assert!(n != 0);
        }
    }
}
fn receive(u: &mut Uart<'_, Uart2<'_>>) -> u8 {
    for _ in 0..100_000_000 {
        if let Ok(b) = u.read_byte() {
            return b;
        }
    }
    panic!("host timeout");
}
fn half(u: &mut Uart<'_, Uart2<'_>>) -> u16 {
    u16::from_le_bytes([receive(u), receive(u)])
}
#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let mut console = unsafe { Uart::from_configured(p.UART2.assume_application_ownership()) };
    emit(&console, b"WS53 EFUSE ready\n");
    let scenario = receive(&mut console);
    let start = receive(&mut console);
    let len = usize::from(half(&mut console));
    let key = receive(&mut console);
    let prefix = usize::from(half(&mut console));
    assert!(len <= 256);
    // The host has established the explicit synthetic profile; no physical handoff.
    let mut source =
        unsafe { EfuseDriver::from_configured(p.EFUSE.assume_application_ownership()) };
    emit(&console, b"WS53 EFUSE attached\n");
    assert_eq!(receive(&mut console), b'G');
    let data = unsafe { &mut *core::ptr::addr_of_mut!(WS53_EFUSE_OUTPUT) };
    let address = EfuseByteAddress::new(u16::from(start)).unwrap();
    let result = source.read_buffer(address, &mut data[1..1 + len]);
    let written = match scenario {
        b'N' | b'X' => {
            assert_eq!(result, Ok(()));
            len
        }
        _ => {
            let e = result.unwrap_err();
            let error = match scenario {
                b'I' => EfuseError::InvalidRange,
                b'M' | b'Q' => EfuseError::ModeMismatch(0x1234),
                b'T' => EfuseError::TimingMismatch(7),
                _ => EfuseError::NotReady,
            };
            assert_eq!((e.completed(), e.error()), (prefix, error));
            prefix
        }
    };
    let mut expected = [0xa5; 258];
    for (i, b) in expected[1..1 + written].iter_mut().enumerate() {
        *b = (usize::from(start) + i)
            .wrapping_mul(73)
            .wrapping_add(usize::from(key)) as u8;
    }
    assert_eq!(data, &expected);
    emit(&console, b"WS53 EFUSE checked\n");
    assert_eq!(receive(&mut console), b'H');
    let token = source.release_without_stopping();
    {
        let mut source = unsafe { EfuseDriver::from_configured(token) };
        assert_eq!(
            source.read_byte(address),
            Ok(start.wrapping_mul(73).wrapping_add(key))
        );
        assert_eq!(source.read_buffer(address, &mut []), Ok(()));
    }
    emit(
        &console,
        b"WS53 PASS EFUSE data=1 prefix=1 retry=1\nWS53 EFUSE done\n",
    );
    loop {
        riscv::asm::wfi();
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
