//! Host-paced real IRQ46/future fixture, ONLY for the opt-in virtual ADMA peer.
#![no_std]
#![no_main]
use core::{
    cell::Cell,
    future::Future,
    pin::Pin,
    ptr,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};
use critical_section::Mutex;
use hisi_hal::{
    Peripherals, interrupt,
    sdio::{
        self, AdmaStorage, AsyncBindError, Buffer, Direction, EventSnapshot, InitProgress,
        Initialization, Table, Transfer, TransferError, TransferProgress,
    },
};
use hisi_riscv_rt::entry;

// cmd, seq, admitted, phase, error, global, fn0, fn1, message-present, message,
// IRQs, wakes, panic, bytes, table, local IRQ enabled, binds, future alive, polls, unused
#[unsafe(no_mangle)]
pub static mut WS53_SDIO_ASYNC_MODEL: [u32; 20] = [0; 20];
#[repr(C, align(4))]
pub struct GuardedData {
    before: [u8; 16],
    bytes: [u8; 64],
    after: [u8; 16],
}
#[unsafe(no_mangle)]
pub static mut WS53_SDIO_ASYNC_DATA: GuardedData = GuardedData {
    before: [0; 16],
    bytes: [0; 64],
    after: [0; 16],
};
#[unsafe(no_mangle)]
pub static mut WS53_SDIO_ASYNC_TABLE: AdmaStorage = AdmaStorage::new();
static IRQS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static WAKES: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static VTABLE: RawWakerVTable = RawWakerVTable::new(|_| raw(), wake, wake, |_| {});
fn raw() -> RawWaker {
    RawWaker::new(ptr::null(), &VTABLE)
}
fn wake(_: *const ()) {
    critical_section::with(|cs| WAKES.borrow(cs).set(WAKES.borrow(cs).get() + 1));
}
#[unsafe(no_mangle)]
extern "C" fn SDIO() {
    // Sole application hart, no control core or other SDIO handler in this model.
    unsafe { sdio::on_async_interrupt() };
    critical_section::with(|cs| IRQS.borrow(cs).set(IRQS.borrow(cs).get() + 1));
}
fn put(i: usize, v: u32) {
    unsafe {
        ptr::addr_of_mut!(WS53_SDIO_ASYNC_MODEL)
            .cast::<u32>()
            .add(i)
            .write_volatile(v)
    };
}
fn get(i: usize) -> u32 {
    unsafe {
        ptr::addr_of!(WS53_SDIO_ASYNC_MODEL)
            .cast::<u32>()
            .add(i)
            .read_volatile()
    }
}
fn telemetry() {
    let counts = critical_section::with(|cs| (IRQS.borrow(cs).get(), WAKES.borrow(cs).get()));
    put(10, counts.0);
    put(11, counts.1);
    put(
        15,
        u32::from(interrupt::is_enabled(interrupt::Interrupt::SDIO)),
    );
}
fn acknowledge() {
    telemetry();
    put(0, 0);
    put(1, get(1) + 1);
}
fn next() -> u32 {
    loop {
        telemetry();
        let c = get(0);
        if c != 0 {
            put(4, 0);
            return c;
        }
        core::hint::spin_loop();
    }
}
fn snapshot(e: EventSnapshot) {
    put(5, e.global);
    put(6, e.function0);
    put(7, e.function1.bits());
    put(8, u32::from(e.host_message.is_some()));
    put(9, e.host_message.unwrap_or(0));
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
fn failure(e: TransferError) {
    put(
        4,
        match e {
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
        },
    );
}
fn bind_failure(e: AsyncBindError) {
    put(
        4,
        match e {
            AsyncBindError::NotActive => 200,
            AsyncBindError::InterruptsEnabled => 201,
        },
    );
}
fn poll<F: Future<Output = Result<sdio::TransferPoll, TransferError>> + Unpin>(f: &mut F) -> bool {
    let waker = unsafe { Waker::from_raw(raw()) };
    put(18, get(18) + 1);
    match Pin::new(f).poll(&mut Context::from_waker(&waker)) {
        Poll::Pending => {
            put(3, 20);
            false
        }
        Poll::Ready(Ok(r)) => {
            snapshot(r.events);
            progress(r.progress);
            true
        }
        Poll::Ready(Err(e)) => {
            failure(e);
            true
        }
    }
}
fn event_loop(t: &mut Transfer<'_>) {
    let mut f = match unsafe { t.wait_async() } {
        Ok(f) => f,
        Err(e) => {
            bind_failure(e);
            acknowledge();
            return;
        }
    };
    put(16, get(16) + 1);
    put(17, 1);
    let mut done = poll(&mut f);
    if done {
        drop(f);
        put(17, 0);
        acknowledge();
        return;
    }
    acknowledge();
    loop {
        let command = next();
        match command {
            5 => done = poll(&mut f),
            6 => {
                drop(f);
                put(17, 0);
                put(3, 21);
                acknowledge();
                return;
            }
            _ => put(4, 106),
        }
        if done {
            drop(f);
            put(17, 0);
            acknowledge();
            return;
        }
        acknowledge();
    }
}

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let token = unsafe { p.SDIO.assume_application_ownership() };
    let mut init = unsafe { Initialization::begin(token) }.unwrap_or_else(|_| panic!("init"));
    put(2, 1);
    put(13, 64);
    put(14, ptr::addr_of_mut!(WS53_SDIO_ASYNC_TABLE) as u32);
    let direction = loop {
        match next() {
            1 => match init.poll() {
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
            },
            c @ (2 | 3) if get(3) == 3 => {
                break if c == 2 {
                    Direction::HostRead
                } else {
                    Direction::HostWrite
                };
            }
            _ => put(4, 103),
        }
        acknowledge();
    };
    let data = unsafe { &mut *ptr::addr_of_mut!(WS53_SDIO_ASYNC_DATA) };
    data.before.fill(0xa5);
    data.after.fill(0xa5);
    for (i, b) in data.bytes.iter_mut().enumerate() {
        *b = if direction == Direction::HostRead {
            (i as u8).wrapping_mul(37).wrapping_add(19)
        } else {
            0xa5
        };
    }
    // The opt-in model supplies coherent, exclusively owned DTCM, a cooperating
    // virtual host and CMD53-length stopping. No equivalent silicon claim.
    let buffer = unsafe { Buffer::from_coherent_static(&mut data.bytes) }
        .unwrap_or_else(|_| panic!("buffer"));
    let table =
        unsafe { Table::from_coherent_static(&mut *ptr::addr_of_mut!(WS53_SDIO_ASYNC_TABLE)) }
            .unwrap_or_else(|_| panic!("table"));
    let mut transfer =
        unsafe { Transfer::new(init.release_without_stopping(), buffer, table, direction) }
            .unwrap_or_else(|_| panic!("transfer"));
    transfer.start().unwrap();
    progress(TransferProgress::WaitingForRequest);
    interrupt::set_threshold(interrupt::Threshold::from_level(0).unwrap());
    unsafe { interrupt::enable_global() };
    acknowledge();
    loop {
        match next() {
            4 => {
                event_loop(&mut transfer);
                continue;
            }
            7 => match unsafe { transfer.wait_async() } {
                Ok(f) => {
                    drop(f);
                    put(3, 22);
                }
                Err(e) => bind_failure(e),
            },
            8 => {
                unsafe { interrupt::enable(interrupt::Interrupt::SDIO) };
                match unsafe { transfer.wait_async() } {
                    Ok(_) => panic!("enabled bind"),
                    Err(e) => bind_failure(e),
                };
                unsafe { interrupt::disable(interrupt::Interrupt::SDIO) };
            }
            9 => match transfer.poll() {
                Ok(r) => {
                    snapshot(r.events);
                    progress(r.progress)
                }
                Err(e) => failure(e),
            },
            _ => put(4, 105),
        }
        acknowledge();
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    put(12, 1);
    loop {
        core::hint::spin_loop();
    }
}
