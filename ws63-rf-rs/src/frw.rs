//! FRW — the WiFi framework's **runtime half**: the message-node pool, the
//! host↔device message FIFO, and the WiFi worker thread.
//!
//! On the single-core WS63 the WiFi stack is split into a host MAC layer
//! (`hmac`) and a device MAC layer (`dmac`) that talk over an in-memory IPC.
//! The vendor blob owns the **protocol half** (`frw_main_init`,
//! `frw_send_msg_to_device`, `frw_event_process_all_event_etc`, …); this module
//! supplies the **runtime half** the blob calls out to:
//!
//! - `frw_fetch_msg_node` / `frw_free_msg_node` — a pool of [`FrwMsgNode`]s
//!   (the C `frw_msg_node`, 40 bytes; its `frw_msg` is at offset 0 so a node
//!   pointer is a valid `frw_msg *`).
//! - [`frw_task_thread`] — the WiFi worker, spawned on the `sched` runtime. It
//!   blocks in [`frw_thread_get_wait`] until a message is posted (by
//!   [`hcc_wifi_msg_send`](crate::hcc::hcc_wifi_msg_send)), drains the FIFO and
//!   dispatches each message to the registered device handler + DMAC hook.
//! - `frw_dmac_msg_hook_register` / `_unregister` — the per-message DMAC tap.
//!
//! Validated standalone (no blob) by `frw_hcc_selftest`: a producer posts N
//! messages through HCC, the worker delivers them to a mock handler in order.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::sched::{self, Semaphore};
use crate::{OSAL_NOK, OSAL_OK};
use core::cell::UnsafeCell;
use core::ffi::{c_int, c_void};
use critical_section as cs;

/// Mirrors C `frw_msg` (16 bytes): a host↔device config/data message.
#[repr(C)]
pub struct FrwMsg {
    /// Input data buffer (caller-allocated, FRW-freed).
    pub data: *mut u8,
    /// Response buffer (interface-allocated, caller-freed) — NULL when async.
    pub rsp: *mut u8,
    /// Input data length.
    pub data_len: u16,
    /// Response buffer length (host→device).
    pub rsp_buf_len: u16,
    /// Actual response length (device→host).
    pub rsp_len: u16,
    /// Packed `sync:1 | type:7 | rsv:8`.
    pub flags: u16,
}

#[repr(C)]
struct OsalListHead {
    next: *mut OsalListHead,
    prev: *mut OsalListHead,
}

/// Mirrors C `frw_msg_node` (40 bytes); `msg` is at offset 0.
#[repr(C)]
pub struct FrwMsgNode {
    /// The message (offset 0 — a `*mut FrwMsgNode` is a valid `frw_msg *`).
    pub msg: FrwMsg,
    list: OsalListHead,
    cb_return: c_int,
    msg_id: u16,
    bits: u8, // wait_cond:1 sync:1 wait_cond_thread:1 pool_used:1 pool_idx:4
    vap_id: u8,
    time_out: u16,
    seq: u16,
    wait_fail: c_int, // osal_atomic { volatile int counter }
}

impl FrwMsgNode {
    const fn zeroed() -> Self {
        FrwMsgNode {
            msg: FrwMsg {
                data: core::ptr::null_mut(),
                rsp: core::ptr::null_mut(),
                data_len: 0,
                rsp_buf_len: 0,
                rsp_len: 0,
                flags: 0,
            },
            list: OsalListHead {
                next: core::ptr::null_mut(),
                prev: core::ptr::null_mut(),
            },
            cb_return: 0,
            msg_id: 0,
            bits: 0,
            vap_id: 0,
            time_out: 0,
            seq: 0,
            wait_fail: 0,
        }
    }
}

/// `void (*)(struct frw_msg *)` — the DMAC message hook / device handler.
pub type MsgHandler = extern "C" fn(*mut FrwMsg);

const POOL_LEN: usize = 24; // C SDK pool_idx is 4 bits; 24 nodes is ample
const FIFO_LEN: usize = 32; // posted-but-not-yet-dispatched messages

/// All FRW runtime state, touched only inside a critical section (single hart).
struct FrwState {
    pool: [FrwMsgNode; POOL_LEN],
    pool_used: [bool; POOL_LEN],
    fifo: [*mut FrwMsgNode; FIFO_LEN],
    fifo_head: usize,
    fifo_count: usize,
    dmac_hook: Option<MsgHandler>,
    device_handler: Option<MsgHandler>,
    running: bool,
    dispatched: u32,
}

struct FrwCell(UnsafeCell<FrwState>);
// SAFETY: every access is inside `cs::with` on a single hart, which serialises.
unsafe impl Sync for FrwCell {}

static FRW: FrwCell = FrwCell(UnsafeCell::new(FrwState {
    pool: [const { FrwMsgNode::zeroed() }; POOL_LEN],
    pool_used: [false; POOL_LEN],
    fifo: [core::ptr::null_mut(); FIFO_LEN],
    fifo_head: 0,
    fifo_count: 0,
    dmac_hook: None,
    device_handler: None,
    running: false,
    dispatched: 0,
}));

/// Posted-message signal: the worker parks on this; a post releases it.
static EVENT: Semaphore = Semaphore::new(0);

#[inline]
fn with_state<R>(f: impl FnOnce(&mut FrwState) -> R) -> R {
    cs::with(|_| {
        // SAFETY: exclusive under the critical section (single hart).
        f(unsafe { &mut *FRW.0.get() })
    })
}

// ── Message-node pool ────────────────────────────────────────────────────────

/// Allocate a zeroed message node from the pool (NULL if exhausted). The
/// returned pointer is also a valid `frw_msg *` (the `msg` field is at offset 0).
#[unsafe(no_mangle)]
pub extern "C" fn frw_fetch_msg_node() -> *mut FrwMsgNode {
    with_state(|s| {
        for i in 0..POOL_LEN {
            if !s.pool_used[i] {
                s.pool_used[i] = true;
                s.pool[i] = FrwMsgNode::zeroed();
                s.pool[i].bits = 0x08 | (i as u8 & 0x0f) << 4; // pool_used + pool_idx
                return core::ptr::addr_of_mut!(s.pool[i]);
            }
        }
        core::ptr::null_mut()
    })
}

/// Return a node to the pool.
#[unsafe(no_mangle)]
pub extern "C" fn frw_free_msg_node(msg: *mut FrwMsgNode) {
    if msg.is_null() {
        return;
    }
    with_state(|s| {
        let base = core::ptr::addr_of!(s.pool[0]) as usize;
        let node = msg as usize;
        let stride = core::mem::size_of::<FrwMsgNode>();
        if node < base {
            return;
        }
        let idx = (node - base) / stride;
        if idx < POOL_LEN && (node - base).is_multiple_of(stride) {
            s.pool_used[idx] = false;
        }
    });
}

// ── Worker thread + dispatch ─────────────────────────────────────────────────

/// Post a node to the worker (called by the HCC transport). Returns `OSAL_OK`,
/// or `OSAL_NOK` if the FIFO is full.
pub(crate) fn post(node: *mut FrwMsgNode) -> c_int {
    let ok = with_state(|s| {
        if s.fifo_count >= FIFO_LEN {
            return false;
        }
        let tail = (s.fifo_head + s.fifo_count) % FIFO_LEN;
        s.fifo[tail] = node;
        s.fifo_count += 1;
        true
    });
    if ok {
        EVENT.up();
        OSAL_OK
    } else {
        OSAL_NOK
    }
}

/// Register the device-side handler the worker delivers messages to (called by
/// `hcc_wifi_msg_register`).
pub(crate) fn set_device_handler(h: Option<MsgHandler>) {
    with_state(|s| s.device_handler = h);
}

fn fifo_pop() -> *mut FrwMsgNode {
    with_state(|s| {
        if s.fifo_count == 0 {
            return core::ptr::null_mut();
        }
        let node = s.fifo[s.fifo_head];
        s.fifo_head = (s.fifo_head + 1) % FIFO_LEN;
        s.fifo_count -= 1;
        node
    })
}

/// Block until the framework has work or is shutting down (`OSAL_OK`).
#[unsafe(no_mangle)]
pub extern "C" fn frw_thread_get_wait() -> c_int {
    EVENT.down();
    OSAL_OK
}

/// The WiFi worker thread. Drains posted messages and dispatches each to the
/// registered device handler then the DMAC hook, until the framework stops.
#[unsafe(no_mangle)]
pub extern "C" fn frw_task_thread(_arg: *mut c_void) -> *mut c_void {
    with_state(|s| s.running = true);
    loop {
        frw_thread_get_wait();
        if !with_state(|s| s.running) {
            break;
        }
        loop {
            let node = fifo_pop();
            if node.is_null() {
                break;
            }
            let (dev, hook) = with_state(|s| (s.device_handler, s.dmac_hook));
            // `msg` is at offset 0 of the node, so the node pointer is the msg
            // pointer (no deref needed).
            let msg = node as *mut FrwMsg;
            if let Some(h) = dev {
                h(msg);
            }
            if let Some(h) = hook {
                h(msg);
            }
            with_state(|s| s.dispatched = s.dispatched.wrapping_add(1));
        }
    }
    core::ptr::null_mut()
}

/// Spawn [`frw_task_thread`] on the scheduler. Internal entry for the runtime /
/// self-test (the blob spawns it via `osal_kthread_create`).
pub(crate) fn start_worker() -> Option<usize> {
    sched::spawn(frw_task_thread, core::ptr::null_mut(), 0)
}

/// Stop the worker (wakes it so it can exit). Internal.
pub(crate) fn stop_worker() {
    with_state(|s| s.running = false);
    EVENT.up();
}

/// Number of messages the worker has dispatched (diagnostic). Internal.
pub(crate) fn dispatched() -> u32 {
    with_state(|s| s.dispatched)
}

// ── Hooks ────────────────────────────────────────────────────────────────────

/// Register the per-message DMAC hook.
#[unsafe(no_mangle)]
pub extern "C" fn frw_dmac_msg_hook_register(hook: Option<MsgHandler>) -> c_int {
    with_state(|s| s.dmac_hook = hook);
    OSAL_OK
}

/// Unregister the DMAC hook.
#[unsafe(no_mangle)]
pub extern "C" fn frw_dmac_msg_hook_unregister(_hook: Option<MsgHandler>) -> c_int {
    with_state(|s| s.dmac_hook = None);
    OSAL_OK
}

// ── Remaining FRW porting hooks (lifecycle / config) ────────────────────────

/// Configure DMAC receive processing. STUB: accepted.
#[unsafe(no_mangle)]
pub extern "C" fn frw_dmac_rcv_cfg(_cfg: *mut c_void) -> c_int {
    OSAL_OK
}
/// Post a received WiFi netbuf up the stack. STUB (netif/smoltcp seam).
#[unsafe(no_mangle)]
pub extern "C" fn frw_rx_wifi_post_netbuf(_netbuf: *mut c_void) -> c_int {
    OSAL_NOK
}
/// Register ROM callbacks. STUB: accepted.
#[unsafe(no_mangle)]
pub extern "C" fn frw_rom_cb_register(_cb: *mut c_void) -> c_int {
    OSAL_OK
}
/// Tear down the HCC service. STUB.
#[unsafe(no_mangle)]
pub extern "C" fn frw_hcc_service_deinit() -> c_int {
    OSAL_OK
}

// ── Software timers (still light stubs — a timer service is future work) ─────

/// STUB: no timer service yet (`osal_adapt_timer_*` are stubbed too).
#[unsafe(no_mangle)]
pub extern "C" fn frw_dmac_timer_init() -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn frw_dmac_timer_exit() -> c_int {
    OSAL_OK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn frw_dmac_timer_timeout_proc() {}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn frw_timer_timeout_proc_event(_arg: core::ffi::c_ulong) {}
