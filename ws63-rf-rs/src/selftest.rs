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

/// Exercise the scheduler-backed OSAL message queue end to end (single task):
/// create a 4×4 queue, write a word, read it back. Returns the round-tripped
/// value (expect `0xCAFE_F00D`). The read goes through the blocking-semaphore
/// path (one item is available, so it does not park). Internal hook.
#[doc(hidden)]
pub fn osal_queue_selftest() -> u32 {
    use core::ffi::{c_uint, c_ulong, c_void};
    let mut qid: c_ulong = 0;
    if crate::osal_queue::osal_msg_queue_create(core::ptr::null(), 4, &mut qid, 0, 4)
        != crate::OSAL_OK
    {
        return 0;
    }
    let tx: u32 = 0xCAFE_F00D;
    crate::osal_queue::osal_msg_queue_write_copy(qid, core::ptr::addr_of!(tx) as *mut c_void, 4, 0);
    let mut rx: u32 = 0;
    let mut sz: c_uint = 4;
    crate::osal_queue::osal_msg_queue_read_copy(
        qid,
        core::ptr::addr_of_mut!(rx) as *mut c_void,
        &mut sz,
        0,
    );
    crate::osal_queue::osal_msg_queue_delete(qid);
    rx
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
