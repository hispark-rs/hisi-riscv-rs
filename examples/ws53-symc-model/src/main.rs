//! Opt-in virtual ownership contract. Never deploy this fixture on real hardware.
#![no_std]
#![no_main]
use core::{
    num::{NonZeroU16, NonZeroU32},
    ptr,
};
use hisi_hal::{
    Peripherals,
    km::{self, CipherKey, CipherSlot, KeyManager, Parity, Usage},
    symc::{
        BlockLength, Buffer, Channel, Completion, DescriptorStorage, Direction, Error, Mode,
        PrepareError, Resources, Table, Transfer,
    },
};
use hisi_riscv_rt::entry;

// 0 command,1 sequence,2 ready,3 phase,4 error,5 algorithm,6 mode,7 decrypt,
// 8 channel,9 slot,10 parity,11 length,12 usage,13 budget,14 panic,15 delay calls,
// 16 input address,17 output address,18 table address,19 completed bytes,
// 20..23 IV,24 KM reserved,25 KM loaded,26 key error,
// 27..30 TEE cause/status REE cause/status,31 unused.
#[unsafe(no_mangle)]
pub static mut WS53_SYMC_MODEL: [u32; 32] = [0; 32];
#[repr(C, align(32))]
pub struct Guarded {
    before: [u8; 32],
    bytes: [u8; 256],
    after: [u8; 32],
}
#[unsafe(no_mangle)]
pub static mut WS53_SYMC_INPUT: Guarded = Guarded {
    before: [0; 32],
    bytes: [0; 256],
    after: [0; 32],
};
#[unsafe(no_mangle)]
pub static mut WS53_SYMC_OUTPUT: Guarded = Guarded {
    before: [0; 32],
    bytes: [0; 256],
    after: [0; 32],
};
#[unsafe(no_mangle)]
pub static mut WS53_SYMC_TABLE: DescriptorStorage = DescriptorStorage::new();

fn put(index: usize, value: u32) {
    unsafe {
        ptr::addr_of_mut!(WS53_SYMC_MODEL)
            .cast::<u32>()
            .add(index)
            .write_volatile(value)
    };
}
fn get(index: usize) -> u32 {
    unsafe {
        ptr::addr_of!(WS53_SYMC_MODEL)
            .cast::<u32>()
            .add(index)
            .read_volatile()
    }
}
fn key_code(error: km::Error) -> u32 {
    match error {
        km::Error::InvalidParity => 1,
        km::Error::Borrowed => 2,
        km::Error::Reserved => 3,
        km::Error::SlotBusy(_) => 4,
        km::Error::Ownership(_) => 5,
        km::Error::Hardware(_) => 6,
        km::Error::FlushStatus(_) => 7,
        km::Error::LoaderBusy => 8,
        km::Error::UnsupportedSelection => 9,
        km::Error::UnexpectedInterrupt(_) => 10,
        km::Error::Acknowledge => 11,
        km::Error::LoadTimeout => 12,
        km::Error::ReleaseTimeout => 13,
        km::Error::ReleaseUnconfirmed(_) => 14,
        km::Error::NotLoaded => 15,
    }
}
fn failure(error: Error) {
    let code = match error {
        Error::NotStarted => 1,
        Error::AlreadyStarted => 2,
        Error::Busy => 3,
        Error::OwnershipLost => 4,
        Error::ClearTimeout => 5,
        Error::Acknowledge => 6,
        Error::Ring => 7,
        Error::Hardware(e) => {
            put(27, e.tee_cause);
            put(28, e.tee_status);
            put(29, e.ree_cause);
            put(30, e.ree_status);
            8
        }
        Error::Timeout => 9,
        Error::Cancelled => 10,
        Error::Key(e) => {
            put(26, key_code(e));
            11
        }
    };
    put(4, code);
    put(3, 4);
}
fn complete(value: Completion) {
    put(3, 3);
    put(19, value.bytes);
    for (i, bytes) in value.iv.as_chunks::<4>().0.iter().enumerate() {
        put(20 + i, u32::from_le_bytes(*bytes));
    }
}
struct Delay;
impl embedded_hal::delay::DelayNs for Delay {
    fn delay_ns(&mut self, _: u32) {
        put(15, get(15) + 1);
        // CPU instructions advance virtual time; this is not a hardware delay.
        for _ in 0..512 {
            unsafe {
                core::arch::asm!("nop", options(nomem, nostack, preserves_flags));
            }
        }
    }
}

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    // Only the explicit model has no competing core, cache, or earlier masters.
    let manager = unsafe { KeyManager::from_configured(p.KM.assume_application_ownership()) };
    let mut spacc = Some(unsafe { p.SPACC.assume_application_ownership() });
    let mut transfer: Option<Transfer<'_>> = None;
    let mut rejected: Option<Resources<'_>> = None;
    let mut prepared = false;
    put(16, unsafe { ptr::addr_of_mut!(WS53_SYMC_INPUT.bytes) }
        as u32);
    put(17, unsafe { ptr::addr_of_mut!(WS53_SYMC_OUTPUT.bytes) }
        as u32);
    put(18, ptr::addr_of_mut!(WS53_SYMC_TABLE) as u32);
    put(2, 1);
    loop {
        let command = get(0);
        if command == 0 {
            core::hint::spin_loop();
            continue;
        }
        put(4, 0);
        match command {
            1 => {
                assert!(!prepared);
                prepared = true;
                let algorithm = get(5);
                let slot = get(9) as usize;
                assert!(algorithm < 4 && slot < 8);
                let mut bytes = [0u8; 32];
                for (i, byte) in bytes.iter_mut().enumerate() {
                    *byte = (i as u8)
                        .wrapping_mul(13)
                        .wrapping_add(slot as u8 * 7)
                        .wrapping_add(3);
                }
                let key = match algorithm {
                    0 => CipherKey::Aes128(bytes[..16].try_into().unwrap()),
                    1 => CipherKey::Aes192(bytes[..24].try_into().unwrap()),
                    2 => CipherKey::Aes256(&bytes),
                    _ => CipherKey::Sm4(bytes[..16].try_into().unwrap()),
                };
                let usage = match get(12) {
                    1 => Usage::Encrypt,
                    2 => Usage::Decrypt,
                    3 => Usage::Both,
                    _ => panic!("usage"),
                };
                let parity = match get(10) {
                    0 => Parity::Even,
                    1 => Parity::Odd,
                    _ => panic!("parity"),
                };
                match manager.load_cipher(
                    CipherSlot::ALL[slot],
                    key,
                    usage,
                    parity,
                    NonZeroU32::new(64).unwrap(),
                    &mut Delay,
                ) {
                    Err(e) => {
                        put(4, 100 + key_code(e));
                        put(3, 4);
                    }
                    Ok(key) => {
                        // Initialize once, before giving static ownership to the HAL.
                        let (input, output, descriptors) = unsafe {
                            let input = &mut *ptr::addr_of_mut!(WS53_SYMC_INPUT);
                            let output = &mut *ptr::addr_of_mut!(WS53_SYMC_OUTPUT);
                            input.before.fill(b'G');
                            input.after.fill(b'H');
                            output.before.fill(b'I');
                            output.after.fill(b'J');
                            output.bytes.fill(b'X');
                            for (i, byte) in input.bytes.iter_mut().enumerate() {
                                *byte = (i as u8)
                                    .wrapping_mul(17)
                                    .wrapping_add(get(6) as u8)
                                    .wrapping_add(slot as u8);
                            }
                            (
                                Buffer::from_coherent_static(&mut input.bytes)
                                    .unwrap_or_else(|_| panic!("input")),
                                Buffer::from_coherent_static(&mut output.bytes)
                                    .unwrap_or_else(|_| panic!("output")),
                                Table::from_coherent_static(&mut *ptr::addr_of_mut!(
                                    WS53_SYMC_TABLE
                                ))
                                .unwrap_or_else(|_| panic!("table")),
                            )
                        };
                        let mut iv = core::array::from_fn(|i| (i as u8) * 3 + slot as u8);
                        if get(6) == 6 {
                            iv.fill(255);
                            iv[15] = 254;
                        }
                        let mode = match get(6) {
                            1 => Mode::Ecb,
                            3 => Mode::Cbc(iv),
                            6 => Mode::Ctr(iv),
                            7 => Mode::Ofb(iv),
                            8 => Mode::Cfb128(iv),
                            _ => panic!("mode"),
                        };
                        let channel = match get(8) {
                            1 => Channel::One,
                            2 => Channel::Two,
                            _ => panic!("channel"),
                        };
                        let direction = match get(7) {
                            0 => Direction::Encrypt,
                            1 => Direction::Decrypt,
                            _ => panic!("direction"),
                        };
                        let resources = Resources {
                            spacc: spacc.take().unwrap(),
                            key,
                            input,
                            output,
                            descriptors,
                        };
                        match unsafe {
                            Transfer::from_configured(
                                resources,
                                channel,
                                mode,
                                direction,
                                BlockLength::from_bytes(get(11) as usize).unwrap(),
                            )
                        } {
                            Ok(t) => {
                                transfer = Some(t);
                                put(3, 1);
                            }
                            Err((e, r)) => {
                                put(
                                    4,
                                    match e {
                                        PrepareError::KeyType => 21,
                                        PrepareError::KeyUsage => 22,
                                        PrepareError::KeyParity => 23,
                                        PrepareError::Capacity => 24,
                                        PrepareError::Overlap => 25,
                                    },
                                );
                                rejected = Some(r);
                                put(3, 4);
                            }
                        }
                    }
                }
            }
            2 => match transfer
                .as_mut()
                .unwrap()
                .start(NonZeroU16::new(get(13) as u16).unwrap(), &mut Delay)
            {
                Ok(()) => put(3, 2),
                Err(e) => failure(e),
            },
            3 => match transfer.as_mut().unwrap().poll() {
                Ok(Some(c)) => complete(c),
                Ok(None) => put(3, 2),
                Err(e) => failure(e),
            },
            4 => match transfer
                .as_mut()
                .unwrap()
                .wait(NonZeroU16::new(get(13) as u16).unwrap(), &mut Delay)
            {
                Ok(c) => complete(c),
                Err(e) => failure(e),
            },
            5 => {
                transfer.as_mut().unwrap().cancel();
                put(3, 5);
            }
            6 => {
                // End the Rust values' lifetimes without a handback operation.
                transfer.take();
                rejected.take();
                put(3, 6);
            }
            7 => {
                // Deliberately exercise forget as a reservation-retention test,
                // even though these current wrappers have no custom Drop impl.
                #[allow(clippy::forget_non_drop)]
                core::mem::forget(transfer.take());
                #[allow(clippy::forget_non_drop)]
                core::mem::forget(rejected.take());
                put(3, 7);
            }
            _ => panic!("command"),
        }
        // Only single-core Rust bookkeeping; no hardware handback or key flush.
        let snapshot = manager.snapshot().unwrap();
        put(24, snapshot.reserved as u32);
        put(25, snapshot.loaded as u32);
        put(0, 0);
        put(1, get(1) + 1);
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    put(14, 1);
    loop {
        core::hint::spin_loop();
    }
}
