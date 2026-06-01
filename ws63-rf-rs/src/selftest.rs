//! Internal scheduler self-test (NOT a public API).
//!
//! Exercises [`crate::sched`] the way the vendor blob's OSAL would: two worker
//! tasks that yield between increments (context switching) and a
//! producer/consumer pair handing off through a blocking semaphore (park/wake).
//! Returned to the `sched_selftest` example so it can report over UART.

use crate::sched::{self, Semaphore};
use core::ffi::c_void;
use portable_atomic::{AtomicU32, Ordering};

const ROUNDS: u32 = 5;
const ITEMS: u32 = 3;

static C0: AtomicU32 = AtomicU32::new(0);
static C1: AtomicU32 = AtomicU32::new(0);
static GOT: AtomicU32 = AtomicU32::new(0);
static DONE: AtomicU32 = AtomicU32::new(0);
/// Producer→consumer handoff (starts empty, so the consumer blocks).
static SEM: Semaphore = Semaphore::new(0);

extern "C" fn worker0(_arg: *mut c_void) -> *mut c_void {
    for _ in 0..ROUNDS {
        C0.fetch_add(1, Ordering::Relaxed);
        sched::yield_now();
    }
    DONE.fetch_add(1, Ordering::Relaxed);
    core::ptr::null_mut()
}
extern "C" fn worker1(_arg: *mut c_void) -> *mut c_void {
    for _ in 0..ROUNDS {
        C1.fetch_add(1, Ordering::Relaxed);
        sched::yield_now();
    }
    DONE.fetch_add(1, Ordering::Relaxed);
    core::ptr::null_mut()
}
extern "C" fn producer(_arg: *mut c_void) -> *mut c_void {
    for _ in 0..ITEMS {
        SEM.up();
        sched::yield_now();
    }
    DONE.fetch_add(1, Ordering::Relaxed);
    core::ptr::null_mut()
}
extern "C" fn consumer(_arg: *mut c_void) -> *mut c_void {
    for _ in 0..ITEMS {
        SEM.down(); // blocks until the producer up()s
        GOT.fetch_add(1, Ordering::Relaxed);
    }
    DONE.fetch_add(1, Ordering::Relaxed);
    core::ptr::null_mut()
}

/// Run the scheduler self-test. Returns `[worker0, worker1, sem_items, done]`;
/// a pass is `[ROUNDS, ROUNDS, ITEMS, 4]`. Internal hook — not a public API.
#[doc(hidden)]
pub fn sched_selftest() -> [u32; 4] {
    sched::init();
    sched::spawn(worker0, core::ptr::null_mut(), 0);
    sched::spawn(worker1, core::ptr::null_mut(), 0);
    sched::spawn(producer, core::ptr::null_mut(), 0);
    sched::spawn(consumer, core::ptr::null_mut(), 0);

    // Drive the scheduler cooperatively from the main task until the 4 spawned
    // tasks finish (bounded so a bug can't hang).
    let mut guard: u32 = 0;
    while DONE.load(Ordering::Relaxed) < 4 && guard < 1_000_000 {
        sched::yield_now();
        guard += 1;
    }
    [
        C0.load(Ordering::Relaxed),
        C1.load(Ordering::Relaxed),
        GOT.load(Ordering::Relaxed),
        DONE.load(Ordering::Relaxed),
    ]
}
