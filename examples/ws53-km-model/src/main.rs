//! Opt-in QEMU-only ownership contract; never deploy on a real board.
#![no_std]
#![no_main]
use core::{num::NonZeroU32, ptr};
use hisi_hal::{
    Peripherals,
    km::{
        Algorithm, CipherKey, CipherSlot, Error, HmacAlgorithm, HmacKey, HmacSlot, KeyManager,
        LoadedKey, Parity, Slot, Usage,
    },
};
use hisi_riscv_rt::entry;

// Host writes only command and parameters 5..10. The CPU creates synthetic
// key bytes, owns all MMIO, and reports reservations even after lost leases.
// 0 cmd,1 seq,2 ready,3 phase,4 error,5 slot,6 algorithm,7 length,8 parity,
// 9 usage,10 poll budget,11 delay calls,12 panic,13 reserved,14 loaded,15 fault,
// 16 lease algorithm,17 slot,18 parity,19 KL error,20 KC error,21 raw detail.
#[unsafe(no_mangle)]
pub static mut WS53_KM_MODEL: [u32; 24] = [0; 24];
fn put(index: usize, value: u32) {
    unsafe {
        ptr::addr_of_mut!(WS53_KM_MODEL)
            .cast::<u32>()
            .add(index)
            .write_volatile(value);
    }
}
fn get(index: usize) -> u32 {
    unsafe {
        ptr::addr_of!(WS53_KM_MODEL)
            .cast::<u32>()
            .add(index)
            .read_volatile()
    }
}
fn code(error: Error) -> u32 {
    match error {
        Error::InvalidParity => 1,
        Error::Borrowed => 2,
        Error::Reserved => 3,
        Error::SlotBusy(_) => 4,
        Error::Ownership(_) => 5,
        Error::Hardware(_) => 6,
        Error::FlushStatus(_) => 7,
        Error::LoaderBusy => 8,
        Error::UnsupportedSelection => 9,
        Error::UnexpectedInterrupt(_) => 10,
        Error::Acknowledge => 11,
        Error::LoadTimeout => 12,
        Error::ReleaseTimeout => 13,
        Error::ReleaseUnconfirmed(_) => 14,
        Error::NotLoaded => 15,
    }
}
fn failure(error: Error) {
    put(4, code(error));
    match error {
        Error::Hardware(e) => {
            put(19, e.loader);
            put(20, e.controller);
        }
        Error::FlushStatus(raw) | Error::UnexpectedInterrupt(raw) => put(21, raw),
        Error::SlotBusy(owner) | Error::Ownership(owner) | Error::ReleaseUnconfirmed(owner) => {
            put(21, owner as u32)
        }
        _ => {}
    }
}
struct Delay;
impl embedded_hal::delay::DelayNs for Delay {
    fn delay_ns(&mut self, _: u32) {
        put(11, get(11) + 1);
        // Progress instructions only, not a calibrated microsecond delay.
        for _ in 0..512 {
            // spin_loop can be empty on this target and the loop optimized
            // away. Explicit NOPs ensure virtual timers get CPU progress.
            unsafe {
                core::arch::asm!("nop", options(nomem, nostack, preserves_flags));
            }
        }
    }
}
#[entry]
fn main() -> ! {
    let token = unsafe {
        Peripherals::take()
            .unwrap()
            .KM
            .assume_application_ownership()
    };
    // Explicit model-only handoff; no hardware capability is asserted here.
    let manager = unsafe { KeyManager::from_configured(token) };
    let mut leases: [Option<LoadedKey<'_>>; 10] = core::array::from_fn(|_| None);
    put(2, 1);
    loop {
        let command = get(0);
        if command == 0 {
            core::hint::spin_loop();
            continue;
        }
        put(4, 0);
        put(19, 0);
        put(20, 0);
        put(21, 0);
        let slot = get(5) as usize;
        assert!(slot < 10);
        let attempts = NonZeroU32::new(get(10)).expect("nonzero test budget");
        match command {
            1 => {
                let mut bytes = [0u8; 128];
                let length = get(7) as usize;
                assert!(length <= 128);
                for (i, byte) in bytes.iter_mut().enumerate() {
                    *byte = (i as u8)
                        .wrapping_mul(17)
                        .wrapping_add(slot as u8)
                        .wrapping_add(length as u8)
                        .wrapping_add(3);
                }
                let algorithm = get(6);
                let result = if algorithm < 4 {
                    assert!(slot < 8);
                    let key = match algorithm {
                        0 => {
                            assert_eq!(length, 16);
                            CipherKey::Aes128(bytes[..16].try_into().unwrap())
                        }
                        1 => {
                            assert_eq!(length, 24);
                            CipherKey::Aes192(bytes[..24].try_into().unwrap())
                        }
                        2 => {
                            assert_eq!(length, 32);
                            CipherKey::Aes256(bytes[..32].try_into().unwrap())
                        }
                        _ => {
                            assert_eq!(length, 16);
                            CipherKey::Sm4(bytes[..16].try_into().unwrap())
                        }
                    };
                    let usage = match get(9) {
                        1 => Usage::Encrypt,
                        2 => Usage::Decrypt,
                        3 => Usage::Both,
                        _ => panic!("usage"),
                    };
                    let parity = match get(8) {
                        0 => Parity::Even,
                        1 => Parity::Odd,
                        _ => panic!("parity"),
                    };
                    Some(manager.load_cipher(
                        CipherSlot::ALL[slot],
                        key,
                        usage,
                        parity,
                        attempts,
                        &mut Delay,
                    ))
                } else {
                    assert!(slot >= 8 && algorithm < 10);
                    let algorithm = [
                        HmacAlgorithm::Sha1,
                        HmacAlgorithm::Sha224,
                        HmacAlgorithm::Sha256,
                        HmacAlgorithm::Sha384,
                        HmacAlgorithm::Sha512,
                        HmacAlgorithm::Sm3,
                    ][algorithm as usize - 4];
                    match HmacKey::new(algorithm, &bytes[..length]) {
                        Ok(key) => Some(manager.load_hmac(
                            HmacSlot::ALL[slot - 8],
                            key,
                            attempts,
                            &mut Delay,
                        )),
                        Err(e) => {
                            put(4, 100);
                            put(20, e.actual as u32);
                            put(21, e.maximum as u32);
                            None
                        }
                    }
                };
                if let Some(result) = result {
                    match result {
                        Ok(key) => {
                            put(
                                16,
                                match key.algorithm() {
                                    Algorithm::Aes128 => 0,
                                    Algorithm::Aes192 => 1,
                                    Algorithm::Aes256 => 2,
                                    Algorithm::Sm4 => 3,
                                    Algorithm::Hmac(a) => {
                                        4 + match a {
                                            HmacAlgorithm::Sha1 => 0,
                                            HmacAlgorithm::Sha224 => 1,
                                            HmacAlgorithm::Sha256 => 2,
                                            HmacAlgorithm::Sha384 => 3,
                                            HmacAlgorithm::Sha512 => 4,
                                            HmacAlgorithm::Sm3 => 5,
                                        }
                                    }
                                },
                            );
                            put(
                                17,
                                match key.slot() {
                                    Slot::Cipher(s) => s as u32,
                                    Slot::Hmac(s) => 8 + s as u32,
                                },
                            );
                            put(18, u32::from(key.parity() == Parity::Odd));
                            leases[slot] = Some(key);
                        }
                        Err(error) => failure(error),
                    }
                }
            }
            2 => match &leases[slot] {
                Some(key) => {
                    if let Err(error) = key.validate() {
                        failure(error);
                    }
                }
                None => put(4, 101),
            },
            3 => match leases[slot].take() {
                // The virtual fixture has no key consumers. This assumption
                // cannot be reused as evidence of real SYMC/Flash quiescence.
                Some(key) => {
                    if let Err(error) =
                        unsafe { key.release_after_external_quiescence(attempts, &mut Delay) }
                    {
                        failure(error);
                    }
                }
                None => put(4, 101),
            },
            4 | 5 => {
                if let Some(key) = leases[slot].take() {
                    if command == 5 {
                        #[allow(clippy::forget_non_drop)]
                        core::mem::forget(key);
                    }
                } else {
                    put(4, 101);
                }
            }
            6 => {}
            _ => panic!("command"),
        }
        let snapshot = manager.snapshot().unwrap();
        put(13, snapshot.reserved as u32);
        put(14, snapshot.loaded as u32);
        put(15, snapshot.fault.map_or(0, code));
        put(3, command);
        put(0, 0);
        put(1, get(1) + 1);
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    put(12, 1);
    loop {
        core::hint::spin_loop();
    }
}
