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

// ── FRW + HCC data-path self-test ───────────────────────────────────────────

static FRW_SENT: AtomicU32 = AtomicU32::new(0);
static FRW_RECV: AtomicU32 = AtomicU32::new(0);
/// XOR of every delivered message's `data_len` — proves payloads arrive intact.
static FRW_CHECK: AtomicU32 = AtomicU32::new(0);

const FRW_N: u32 = 5;

/// Mock device handler: the worker delivers each posted message here. Folds the
/// payload into a checksum, counts it, and returns the node to the pool.
extern "C" fn frw_mock_handler(msg: *mut crate::frw::FrwMsg) {
    if !msg.is_null() {
        // SAFETY: `msg` is a live node from frw_fetch_msg_node (msg at offset 0).
        let dlen = unsafe { (*msg).data_len } as u32;
        FRW_CHECK.fetch_xor(dlen, Ordering::Relaxed);
        FRW_RECV.fetch_add(1, Ordering::Relaxed);
    }
    crate::frw::frw_free_msg_node(msg as *mut crate::frw::FrwMsgNode);
}

/// Exercise the FRW/HCC data path end to end (no blob): register a handler,
/// spawn the worker, post `FRW_N` messages through HCC, and confirm the worker
/// delivers them all. Returns `[sent, received, dispatched, checksum_ok]`; a
/// pass is `[FRW_N, FRW_N, FRW_N, 1]`. Internal hook.
#[doc(hidden)]
pub fn frw_hcc_selftest() -> [u32; 4] {
    use crate::frw::FrwMsg;
    sched::init();
    crate::hcc::hcc_wifi_msg_register(Some(frw_mock_handler));
    crate::frw::start_worker();

    let mut expect: u32 = 0;
    for i in 0..FRW_N {
        let node = crate::frw::frw_fetch_msg_node();
        if node.is_null() {
            break;
        }
        let dlen = 0x100 + i;
        // SAFETY: fresh node from the pool.
        unsafe {
            (*node).msg.data_len = dlen as u16;
        }
        expect ^= dlen;
        FRW_SENT.fetch_add(1, Ordering::Relaxed);
        crate::hcc::hcc_wifi_msg_send(node as *mut FrwMsg);
        sched::yield_now(); // let the worker drain
    }

    let mut guard: u32 = 0;
    while crate::frw::dispatched() < FRW_N && guard < 1_000_000 {
        sched::yield_now();
        guard += 1;
    }
    crate::frw::stop_worker();
    for _ in 0..16 {
        sched::yield_now(); // let the worker observe the stop and exit
    }
    [
        FRW_SENT.load(Ordering::Relaxed),
        FRW_RECV.load(Ordering::Relaxed),
        crate::frw::dispatched(),
        (FRW_CHECK.load(Ordering::Relaxed) == expect) as u32,
    ]
}

// ── Software-timer self-test ─────────────────────────────────────────────────

static TIMER_FIRED: AtomicU32 = AtomicU32::new(0);

extern "C" fn timer_cb(_data: core::ffi::c_ulong) {
    TIMER_FIRED.fetch_add(1, Ordering::Relaxed);
}

/// Exercise the software-timer service deterministically (no scheduler needed):
/// arm a one-shot 2 ms timer, drive `frw_dmac_timer_timeout_proc` until it
/// fires, confirm it does NOT re-fire on its own, then re-arm and confirm it
/// fires again. Returns `[after_oneshot, after_rearm, ok]`; a pass is `[1,2,1]`.
/// Internal hook.
#[doc(hidden)]
pub fn timer_selftest() -> [u32; 3] {
    use crate::timer::{self, OsalTimer};
    TIMER_FIRED.store(0, Ordering::Relaxed);
    timer::frw_dmac_timer_init();

    let mut t = OsalTimer {
        timer: core::ptr::null_mut(),
        handler: None,
        data: 0,
        interval: 0,
    };
    // osal_adapt_timer_init takes the callback as a `void*` (the blob passes a
    // fn pointer the same way).
    let func = timer_cb as extern "C" fn(core::ffi::c_ulong) as *mut core::ffi::c_void;
    timer::osal_adapt_timer_init(&mut t, func, 0, 2);
    timer::osal_adapt_timer_mod(&mut t, 2);

    // Drive the timer service; mcycle (the time base) advances as we spin.
    let mut guard: u32 = 0;
    while TIMER_FIRED.load(Ordering::Relaxed) == 0 && guard < 50_000_000 {
        timer::frw_dmac_timer_timeout_proc();
        guard += 1;
    }
    let after_oneshot = TIMER_FIRED.load(Ordering::Relaxed);

    // A one-shot must NOT re-fire without re-arming.
    for _ in 0..10_000 {
        timer::frw_dmac_timer_timeout_proc();
    }

    // Re-arm; it must fire again.
    timer::osal_adapt_timer_mod(&mut t, 2);
    guard = 0;
    while TIMER_FIRED.load(Ordering::Relaxed) < 2 && guard < 50_000_000 {
        timer::frw_dmac_timer_timeout_proc();
        guard += 1;
    }
    let after_rearm = TIMER_FIRED.load(Ordering::Relaxed);
    timer::osal_adapt_timer_destroy(&mut t);

    [
        after_oneshot,
        after_rearm,
        (after_oneshot == 1 && after_rearm == 2) as u32,
    ]
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
