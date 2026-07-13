//! FRW — the WiFi framework's **runtime half**: the message-node pool, the
//! host↔device message FIFO, and the WiFi worker thread.
//!
//! On the single-core WS63 the WiFi stack is split into a host MAC layer
//! (`hmac`) and a device MAC layer (`dmac`) that talk over an in-memory IPC.
//! The vendor blob owns the **protocol half** (`frw_main_init`,
//! `frw_send_msg_to_device`, `frw_event_process_all_event_etc`, …); this module
//! supplies the **runtime half** the blob calls out to:
//!
//! - an internal message-node pool for the standalone FRW/HCC self-test
//!   (the device C `frw_msg_node`, 40 bytes; its `frw_msg` is at offset 0 so a node
//!   pointer is a valid `frw_msg *`).
//! - [`local_task_thread`] — the self-test worker, spawned on the `sched` runtime.
//!   It blocks until a message is posted by the local HCC self-test transport, drains the FIFO and
//!   dispatches each message to the registered device handler + DMAC hook.
//!
//! Per-message DMAC hooks remain owned by the mask-ROM
//! `frw_dmac_msg_hook_register(msg_id, callback)` table. This port must not
//! override that symbol with the single-handler HCC seam.
//!
//! Validated standalone (no blob) by `frw_hcc_selftest`: a producer posts N
//! messages through HCC, the worker delivers them to a mock handler in order.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::runtime;
use crate::{OSAL_NOK, OSAL_OK};
use core::cell::UnsafeCell;
use core::ffi::{c_int, c_void};
use critical_section as cs;
use hisi_rf_rtos_driver::{Semaphore, WaitTimeout};

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

/// Mirrors the device C `frw_msg_node` (40 bytes); `msg` is at offset 0.
///
/// This is only used by the Rust-owned standalone self-test pool. The vendor
/// host path allocates its distinct 44-byte node (with `wait_q` at `0x28`)
/// itself, while the ROM owns the production device node pool.
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

#[cfg(target_pointer_width = "32")]
const _: () = assert!(core::mem::size_of::<FrwMsgNode>() == 40);

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

const POOL_LEN: usize = 16; // device pool_idx is four bits
const FIFO_LEN: usize = 32; // posted-but-not-yet-dispatched messages

/// All FRW runtime state, touched only inside a critical section (single hart).
struct FrwState {
    pool: [FrwMsgNode; POOL_LEN],
    pool_used: [bool; POOL_LEN],
    fifo: [*mut FrwMsgNode; FIFO_LEN],
    fifo_head: usize,
    fifo_count: usize,
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
pub(crate) fn local_fetch_msg_node() -> *mut FrwMsgNode {
    with_state(|s| {
        for i in 0..POOL_LEN {
            if !s.pool_used[i] {
                s.pool_used[i] = true;
                s.pool[i] = FrwMsgNode::zeroed();
                s.pool[i].bits = 0x08 | (i as u8 & 0x0f) << 4;
                return core::ptr::addr_of_mut!(s.pool[i]);
            }
        }
        core::ptr::null_mut()
    })
}

/// Return a node to the pool.
pub(crate) fn local_free_msg_node(msg: *mut FrwMsgNode) {
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
        let _ = EVENT.up();
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

/// The WiFi worker thread. Drains posted messages and dispatches each to the
/// registered device handler then the DMAC hook, until the framework stops.
extern "C" fn local_task_thread(_arg: *mut c_void) -> *mut c_void {
    with_state(|s| s.running = true);
    loop {
        // Park until a message is posted OR the nearest timer is due (so timers
        // fire even with no message traffic). u32::MAX == no timer armed.
        let _ = EVENT.down_timeout(WaitTimeout::from_millis(crate::timer::next_delay_ms()));
        if !with_state(|s| s.running) {
            break;
        }
        // Fire any expired software timers (cooperative, from this thread).
        crate::timer::local_timer_timeout_proc();
        loop {
            let node = fifo_pop();
            if node.is_null() {
                break;
            }
            let dev = with_state(|s| s.device_handler);
            // `msg` is at offset 0 of the node, so the node pointer is the msg
            // pointer (no deref needed).
            let msg = node as *mut FrwMsg;
            if let Some(h) = dev {
                h(msg);
            }
            with_state(|s| s.dispatched = s.dispatched.wrapping_add(1));
        }
    }
    core::ptr::null_mut()
}

/// Spawn [`local_task_thread`] for the standalone Rust self-test.
pub(crate) fn start_worker() -> Option<usize> {
    runtime::spawn(local_task_thread, core::ptr::null_mut(), 0)
}

/// Stop the worker (wakes it so it can exit). Internal.
pub(crate) fn stop_worker() {
    with_state(|s| s.running = false);
    let _ = EVENT.up();
}

/// Number of messages the worker has dispatched (diagnostic). Internal.
pub(crate) fn dispatched() -> u32 {
    with_state(|s| s.dispatched)
}

// The local software-timer model is selftest-only. Production device FRW timer
// entry points stay owned by mask ROM and are not exported from this crate.
