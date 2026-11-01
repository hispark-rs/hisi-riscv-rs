//! Hardware-only observation of SDK EFUSE_SEC_VERIFY_ENABLE (bit 1168).
//! No provisioning, OTP writes, power/timing initialization or key reads.
use hisi_hal::{
    efuse::{EfuseByteAddress, EfuseDriver},
    peripherals::{Efuse, Uart2, Unclaimed},
    uart::Uart,
};

pub fn check(uart: &Uart<'_, Uart2<'_>>, token: Unclaimed<Efuse<'_>>) {
    // The runner holds the control CPU in reset, masks application interrupts,
    // stops competing users and checks boot-done, read mode/timing and inactive
    // programming supplies/switches. Inherit the qualified SDK read profile;
    // never initialize or repair it here. The HAL's sole command is READ 5a5a.
    let mut efuse = unsafe { EfuseDriver::from_configured(token.assume_application_ownership()) };
    let address = EfuseByteAddress::new(1168 / 8).unwrap();
    let mut observed = [0u8; 8];
    for value in &mut observed {
        *value = efuse.read_byte(address).unwrap() & 1;
    }
    assert!(observed.iter().all(|&value| value == observed[0]));
    // Only this public policy bit is logged; discard the other bits in the byte.
    super::emit(uart, b"WS53 SEC_VERIFY samples=");
    for value in observed {
        super::emit(uart, &[b'0' + value]);
    }
    super::emit(uart, b"\r\nWS53 SEC_VERIFY READ DONE\r\n");
}
