//! Explicit QEMU handoff fixture. Never deploy this ownership assumption on silicon.
#![no_std]
#![no_main]
use core::{num::NonZeroU16, ptr};
use hisi_hal::{
    Peripherals,
    hash::{
        Algorithm, Channel, DescriptorStorage, Digest, Error, FormatError, Input, Table, Transfer,
    },
};
use hisi_riscv_rt::entry;

// command, sequence, ready, phase, error, algorithm, channel, message length,
// digest length, TEE cause, REE cause, delay calls, panic, encoded length,
// input address, table address, digest[16 words], TEE status, REE status,
// message kind (0=pattern, 1=abc, 2=abcd repeated), reserved[5].
#[unsafe(no_mangle)]
pub static mut WS53_HASH_MODEL: [u32; 40] = [0; 40];
#[repr(C, align(32))]
pub struct GuardedData {
    before: [u8; 32],
    bytes: [u8; 256],
    after: [u8; 32],
}
#[unsafe(no_mangle)]
pub static mut WS53_HASH_DATA: GuardedData = GuardedData {
    before: [0; 32],
    bytes: [0; 256],
    after: [0; 32],
};
#[unsafe(no_mangle)]
pub static mut WS53_HASH_TABLE: DescriptorStorage = DescriptorStorage::new();

fn put(index: usize, value: u32) {
    unsafe {
        ptr::addr_of_mut!(WS53_HASH_MODEL)
            .cast::<u32>()
            .add(index)
            .write_volatile(value);
    }
}
fn get(index: usize) -> u32 {
    unsafe {
        ptr::addr_of!(WS53_HASH_MODEL)
            .cast::<u32>()
            .add(index)
            .read_volatile()
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
            put(9, e.tee_cause);
            put(10, e.ree_cause);
            put(32, e.tee_status);
            put(33, e.ree_status);
            8
        }
        Error::Timeout => 9,
        Error::Cancelled => 10,
    };
    put(4, code);
}
fn completed(digest: Digest) {
    put(3, 4);
    put(8, digest.as_bytes().len() as u32);
    for (i, bytes) in digest.as_bytes().as_chunks::<4>().0.iter().enumerate() {
        put(16 + i, u32::from_le_bytes(*bytes));
    }
}
struct Delay;
impl embedded_hal::delay::DelayNs for Delay {
    fn delay_ns(&mut self, _: u32) {
        put(11, get(11) + 1);
        // Model-only CPU progress, not a calibrated delay provider. SPACC's
        // virtual timers run independently; bounds are attempt counts.
        for _ in 0..128 {
            core::hint::spin_loop();
        }
    }
}

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    // QEMU opt-in profile has no competing control core or prior DMA. This is
    // not a board IPC/ownership protocol and does not establish physical drain.
    let mut token = Some(unsafe { p.SPACC.assume_application_ownership() });
    let mut transfer: Option<Transfer> = None;
    put(14, unsafe { ptr::addr_of_mut!(WS53_HASH_DATA.bytes) }
        as u32);
    put(15, ptr::addr_of_mut!(WS53_HASH_TABLE) as u32);
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
                let algorithm = match get(5) {
                    0 => Algorithm::Sha1,
                    1 => Algorithm::Sha224,
                    2 => Algorithm::Sha256,
                    3 => Algorithm::Sha384,
                    4 => Algorithm::Sha512,
                    5 => Algorithm::Sm3,
                    _ => panic!("invalid fixture algorithm"),
                };
                let channel = match get(6) {
                    1 => Channel::One,
                    2 => Channel::Two,
                    _ => panic!("invalid fixture channel"),
                };
                let length = get(7) as usize;
                assert!(length <= 256);
                if let Some(token) = token.take() {
                    let mut message = [0u8; 256];
                    for (i, byte) in message[..length].iter_mut().enumerate() {
                        *byte = match get(34) {
                            0 => (i as u8).wrapping_mul(37).wrapping_add(11),
                            1 if length == 3 => b"abc"[i],
                            2 => b"abcd"[i % 4],
                            _ => panic!("invalid fixture message kind"),
                        };
                    }
                    // Access once, before transferring ownership. All future
                    // commands retain these static allocations, even on error.
                    let data = unsafe { &mut *ptr::addr_of_mut!(WS53_HASH_DATA) };
                    data.before.fill(0xa5);
                    data.bytes.fill(0xa5);
                    data.after.fill(0xa5);
                    let input = match unsafe { Input::from_coherent_static(&mut data.bytes) } {
                        Ok(v) => v,
                        Err(_) => panic!("input memory contract"),
                    };
                    match input.prepare(algorithm, &message[..length]) {
                        Ok(input) => {
                            put(13, input.encoded_len());
                            let storage = unsafe { &mut *ptr::addr_of_mut!(WS53_HASH_TABLE) };
                            let table = match unsafe { Table::from_coherent_static(storage) } {
                                Ok(v) => v,
                                Err(_) => panic!("table memory contract"),
                            };
                            // Explicit opt-in model contract covers the ACK,
                            // ring, inherited clock and coherent memory profile.
                            transfer = Some(unsafe {
                                Transfer::from_configured(token, channel, input, table)
                            });
                            put(3, 1);
                        }
                        Err((error, _retained)) => put(
                            4,
                            match error {
                                FormatError::Length => 21,
                                FormatError::Capacity => 22,
                                FormatError::ProfileLength => 23,
                            },
                        ),
                    }
                } else {
                    put(4, 101);
                }
            }
            2..=5 => {
                if let Some(t) = transfer.as_mut() {
                    match command {
                        2 => match t.start(NonZeroU16::new(255).unwrap(), &mut Delay) {
                            Ok(()) => put(3, 2),
                            Err(e) => failure(e),
                        },
                        3 => match t.poll() {
                            Ok(Some(digest)) => completed(digest),
                            Ok(None) => put(3, 3),
                            Err(e) => failure(e),
                        },
                        4 => match t.wait(NonZeroU16::new(255).unwrap(), &mut Delay) {
                            Ok(digest) => completed(digest),
                            Err(e) => failure(e),
                        },
                        5 => {
                            t.cancel();
                            put(3, 5);
                        }
                        _ => unreachable!(),
                    }
                } else {
                    put(4, 102);
                }
            }
            6 | 7 => {
                if let Some(t) = transfer.take() {
                    if command == 7 {
                        // Exercise the public forget contract even though the
                        // current type intentionally has no Drop implementation.
                        #[allow(clippy::forget_non_drop)]
                        core::mem::forget(t);
                    }
                    // No buffer/table access, hardware stop or reclamation.
                    put(3, command);
                } else {
                    put(4, 102);
                }
            }
            _ => put(4, 103),
        }
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
