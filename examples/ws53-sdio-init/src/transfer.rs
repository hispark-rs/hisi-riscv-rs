//! Host-paced retained ADMA fixture. Model handoff only; never deploy on a board.
#![no_std]
#![no_main]
use core::{num::NonZeroU16, ptr};
use hisi_hal::{
    Peripherals,
    sdio::{
        AdmaStorage, Buffer, Direction, EventSnapshot, InitProgress, Initialization, Table,
        Transfer, TransferError, TransferProgress,
    },
};
use hisi_riscv_rt::entry;

// [command, sequence, admitted, phase, error, global, fn0, fn1, message-present,
// message, notifications, delay-calls, panic, bytes, table-address, reserved]
#[unsafe(no_mangle)]
pub static mut WS53_SDIO_TRANSFER_MODEL: [u32; 16] = [0; 16];
#[repr(C, align(4))]
pub struct GuardedData {
    before: [u8; 16],
    bytes: [u8; 64],
    after: [u8; 16],
}
#[unsafe(no_mangle)]
pub static mut WS53_SDIO_TRANSFER_DATA: GuardedData = GuardedData {
    before: [0; 16],
    bytes: [0; 64],
    after: [0; 16],
};
#[unsafe(no_mangle)]
pub static mut WS53_SDIO_TRANSFER_TABLE: AdmaStorage = AdmaStorage::new();

fn put(i: usize, v: u32) {
    unsafe {
        ptr::addr_of_mut!(WS53_SDIO_TRANSFER_MODEL)
            .cast::<u32>()
            .add(i)
            .write_volatile(v);
    }
}
fn get(i: usize) -> u32 {
    unsafe {
        ptr::addr_of!(WS53_SDIO_TRANSFER_MODEL)
            .cast::<u32>()
            .add(i)
            .read_volatile()
    }
}
fn snapshot(e: EventSnapshot) {
    put(5, e.global);
    put(6, e.function0);
    put(7, e.function1.bits());
    put(8, u32::from(e.host_message.is_some()));
    put(9, e.host_message.unwrap_or(0));
}
fn failure(e: TransferError) {
    let code = match e {
        TransferError::NotStarted => 1,
        TransferError::AlreadyStarted => 2,
        TransferError::Retained => 3,
        TransferError::ClockGate => 4,
        TransferError::Configuration => 5,
        TransferError::DirtyStart(e) => {
            snapshot(e);
            6
        }
        TransferError::Request { events, .. } => {
            snapshot(events);
            7
        }
        TransferError::LengthMismatch { events, .. } => {
            snapshot(events);
            8
        }
        TransferError::Unsupported(e) => {
            snapshot(e);
            9
        }
        TransferError::DeviceError(e) => {
            snapshot(e);
            10
        }
        TransferError::Timeout => 11,
    };
    put(4, code);
}
fn progress(p: TransferProgress) {
    put(
        3,
        match p {
            TransferProgress::WaitingForRequest => 10,
            TransferProgress::Running => 11,
            TransferProgress::CompletionObserved => 12,
        },
    );
}
struct Delay;
impl embedded_hal::delay::DelayNs for Delay {
    fn delay_ns(&mut self, _: u32) {
        put(11, get(11) + 1);
    }
}

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    // Explicit model contract: no control core, competing user or prior DMA.
    let token = unsafe { p.SDIO.assume_application_ownership() };
    let initial = match unsafe { Initialization::begin(token) } {
        Ok(v) => v,
        Err(_) => {
            put(4, 100);
            loop {
                core::hint::spin_loop();
            }
        }
    };
    let mut initial = Some(initial);
    let mut transfer: Option<Transfer<'static>> = None;
    put(2, 1);
    put(13, 64);
    put(14, ptr::addr_of_mut!(WS53_SDIO_TRANSFER_TABLE) as u32);
    loop {
        let command = get(0);
        if command == 0 {
            core::hint::spin_loop();
            continue;
        }
        put(4, 0);
        match command {
            1 => {
                if let Some(v) = initial.as_mut() {
                    match v.poll() {
                        Ok(r) => {
                            snapshot(r.events);
                            put(
                                3,
                                match r.progress {
                                    InitProgress::WaitingHostClock => 1,
                                    InitProgress::WaitingFunctionEnable => 2,
                                    InitProgress::FunctionReady => 3,
                                },
                            );
                        }
                        Err(_) => put(4, 101),
                    }
                } else {
                    put(4, 102);
                }
            }
            2 | 3 => {
                if get(3) != 3 || transfer.is_some() {
                    put(4, 103);
                } else if let Some(initial) = initial.take() {
                    let direction = if command == 2 {
                        Direction::HostRead
                    } else {
                        Direction::HostWrite
                    };
                    // Fill only before static ownership is handed to ADMA.
                    let data = unsafe { &mut *ptr::addr_of_mut!(WS53_SDIO_TRANSFER_DATA) };
                    data.before.fill(0xa5);
                    data.after.fill(0xa5);
                    for (i, b) in data.bytes.iter_mut().enumerate() {
                        *b = if command == 2 {
                            (i as u8).wrapping_mul(37).wrapping_add(19)
                        } else {
                            0xa5
                        };
                    }
                    let buffer = match unsafe { Buffer::from_coherent_static(&mut data.bytes) } {
                        Ok(v) => v,
                        Err(_) => panic!("buffer range"),
                    };
                    let storage = unsafe { &mut *ptr::addr_of_mut!(WS53_SDIO_TRANSFER_TABLE) };
                    let table = match unsafe { Table::from_coherent_static(storage) } {
                        Ok(v) => v,
                        Err(_) => panic!("table range"),
                    };
                    // The opt-in QEMU profile supplies accessible coherent RAM,
                    // a coordinated host and command-length descriptor stopping.
                    let mut t = match unsafe {
                        Transfer::new(initial.release_without_stopping(), buffer, table, direction)
                    } {
                        Ok(v) => v,
                        Err(_) => panic!("allocation overlap"),
                    };
                    match t.start() {
                        Ok(()) => progress(TransferProgress::WaitingForRequest),
                        Err(e) => failure(e),
                    }
                    transfer = Some(t);
                }
            }
            4 => {
                if let Some(t) = transfer.as_mut() {
                    match t.poll() {
                        Ok(r) => {
                            snapshot(r.events);
                            progress(r.progress)
                        }
                        Err(e) => failure(e),
                    }
                } else {
                    put(4, 104);
                }
            }
            5 => {
                if let Some(t) = transfer.as_mut() {
                    match t.wait_bounded(NonZeroU16::new(7).unwrap(), &mut Delay, |e| {
                        snapshot(e);
                        put(10, get(10) + 1);
                    }) {
                        Ok(e) => {
                            snapshot(e);
                            progress(TransferProgress::CompletionObserved)
                        }
                        Err(e) => failure(e),
                    }
                } else {
                    put(4, 104);
                }
            }
            6 => {
                if let Some(t) = transfer.as_mut() {
                    t.cancel();
                } else {
                    put(4, 104);
                }
            }
            _ => put(4, 105),
        }
        // No CPU buffer read or reclamation, even after observed completion.
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
