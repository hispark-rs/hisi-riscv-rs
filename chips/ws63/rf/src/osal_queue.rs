//! Scheduler-backed OSAL message queues + event groups.
//!
//! C SDK `osal_msg_queue.h` / `osal_event.h` (deeper OSAL, beyond
//! `ws63-RF/port_*.h`). A queue is a heap ring buffer + a counting
//! `Semaphore` for blocking reads; an event group is a bitmask + a semaphore
//! the reader rechecks. Handles: the queue id is the heap object address; the
//! event handle field holds it.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::{OSAL_NOK, OSAL_OK};
use core::cell::RefCell;
use core::ffi::{c_char, c_int, c_uint, c_ulong, c_void};
use critical_section::Mutex;
use hisi_rf_rtos_driver::{Semaphore, WaitOutcome, WaitTimeout};

// ── Message queue (bounded ring + counting semaphore) ───────────────────────

struct MsgQueue {
    items: Semaphore, // counts queued items (blocks readers)
    ring: *mut u8,
    item_size: usize,
    cap: usize,
    head: usize,
    count: usize,
}

/// Create a message queue. Stores the handle in `*queue_id`.
#[unsafe(no_mangle)]
pub extern "C" fn osal_msg_queue_create(
    _name: *const c_char,
    queue_len: u16,
    queue_id: *mut c_ulong,
    _flags: c_uint,
    max_msgsize: u16,
) -> c_int {
    if queue_id.is_null() || queue_len == 0 || max_msgsize == 0 {
        return OSAL_NOK;
    }
    let cap = queue_len as usize;
    let isz = max_msgsize as usize;
    let ring = crate::alloc::osal_kmalloc(cap * isz) as *mut u8;
    if ring.is_null() {
        return OSAL_NOK;
    }
    let q = crate::alloc::osal_kmalloc(core::mem::size_of::<MsgQueue>()) as *mut MsgQueue;
    if q.is_null() {
        crate::alloc::osal_kfree(ring as *mut c_void);
        return OSAL_NOK;
    }
    // SAFETY: freshly allocated, correctly sized.
    unsafe {
        q.write(MsgQueue {
            items: Semaphore::new(0),
            ring,
            item_size: isz,
            cap,
            head: 0,
            count: 0,
        });
        *queue_id = q as c_ulong;
    }
    OSAL_OK
}

/// Enqueue a copy of `[buffer_addr, +buffer_size)` (clamped to the item size).
#[unsafe(no_mangle)]
pub extern "C" fn osal_msg_queue_write_copy(
    queue_id: c_ulong,
    buffer_addr: *mut c_void,
    buffer_size: c_uint,
    _timeout: c_uint,
) -> c_int {
    let q = queue_id as *mut MsgQueue;
    if q.is_null() || buffer_addr.is_null() {
        return OSAL_NOK;
    }
    let ok = critical_section::with(|_cs| {
        // SAFETY: q is a live handle; exclusive under the critical section.
        let m = unsafe { &mut *q };
        if m.count >= m.cap {
            return false;
        }
        let n = (buffer_size as usize).min(m.item_size);
        let slot = (m.head + m.count) % m.cap;
        unsafe {
            core::ptr::copy_nonoverlapping(
                buffer_addr.cast::<u8>(),
                m.ring.add(slot * m.item_size),
                n,
            )
        };
        m.count += 1;
        true
    });
    if ok {
        // SAFETY: q is a live handle.
        if unsafe { (*q).items.up() }.is_ok() {
            OSAL_OK
        } else {
            OSAL_NOK
        }
    } else {
        OSAL_NOK
    }
}

/// Dequeue one item; copies up to `*buffer_size`. Blocks up to `timeout` ms
/// (`u32::MAX` == wait-forever) for an item; returns `OSAL_NOK` on timeout.
#[unsafe(no_mangle)]
pub extern "C" fn osal_msg_queue_read_copy(
    queue_id: c_ulong,
    buffer_addr: *mut c_void,
    buffer_size: *mut c_uint,
    timeout: c_uint,
) -> c_int {
    let q = queue_id as *mut MsgQueue;
    if q.is_null() || buffer_addr.is_null() {
        return OSAL_NOK;
    }
    // SAFETY: q is a live handle. Block (up to `timeout`) for an item.
    if !matches!(
        unsafe { (*q).items.down_timeout(WaitTimeout::from_millis(timeout)) },
        Ok(WaitOutcome::Acquired)
    ) {
        return OSAL_NOK;
    }
    critical_section::with(|_cs| {
        let m = unsafe { &mut *q };
        if m.count == 0 {
            return; // shouldn't happen after a successful down()
        }
        let want = if buffer_size.is_null() {
            m.item_size
        } else {
            (unsafe { *buffer_size } as usize).min(m.item_size)
        };
        unsafe {
            core::ptr::copy_nonoverlapping(
                m.ring.add(m.head * m.item_size),
                buffer_addr.cast::<u8>(),
                want,
            );
            if !buffer_size.is_null() {
                *buffer_size = want as c_uint;
            }
        }
        m.head = (m.head + 1) % m.cap;
        m.count -= 1;
    });
    OSAL_OK
}

/// 1 if the queue is full, else 0.
#[unsafe(no_mangle)]
pub extern "C" fn osal_msg_queue_is_full(queue_id: c_ulong) -> c_int {
    let q = queue_id as *mut MsgQueue;
    if q.is_null() {
        return 0;
    }
    critical_section::with(|_cs| {
        let m = unsafe { &*q };
        (m.count >= m.cap) as c_int
    })
}

/// Delete a message queue.
#[unsafe(no_mangle)]
pub extern "C" fn osal_msg_queue_delete(queue_id: c_ulong) {
    let q = queue_id as *mut MsgQueue;
    if q.is_null() {
        return;
    }
    // SAFETY: q is a live handle, deleted once.
    unsafe {
        let ring = (*q).ring;
        if !ring.is_null() {
            crate::alloc::osal_kfree(ring as *mut c_void);
        }
        // SAFETY: deletion requires all producers/consumers to be quiesced.
        let _ = (*q).items.destroy();
    }
    crate::alloc::osal_kfree(q as *mut c_void);
}

// ── Event group (bitmask + per-waiter wakeup) ───────────────────────────────

const WAITMODE_AND: c_uint = 4; // all bits (OR / any is the default else-branch)
const WAITMODE_CLR: c_uint = 1; // clear matched bits on success

struct EventGroup {
    bits: u32,
    waiters: [EventWaiter; MAX_EVENT_WAITERS],
    reads: u32,
    writes: u32,
    matches: u32,
    last_read_mask: u32,
    last_write_mask: u32,
    last_mode: u32,
}

const MAX_EVENT_WAITERS: usize = 8;

#[derive(Clone, Copy)]
struct EventWaiter {
    semaphore: usize,
    mask: u32,
    mode: u32,
}

impl EventWaiter {
    const EMPTY: Self = Self {
        semaphore: 0,
        mask: 0,
        mode: 0,
    };
}

const MAX_EVENT_GROUPS: usize = 16;
static EVENT_GROUPS: Mutex<RefCell<[usize; MAX_EVENT_GROUPS]>> =
    Mutex::new(RefCell::new([0; MAX_EVENT_GROUPS]));

/// Read-only event-group state used while matching the Rust OSAL to LiteOS.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EventDiagnostic {
    pub event: usize,
    pub bits: u32,
    pub reads: u32,
    pub writes: u32,
    pub matches: u32,
    pub last_read_mask: u32,
    pub last_write_mask: u32,
    pub last_mode: u32,
}

fn register_event_group(group: *mut EventGroup) -> bool {
    critical_section::with(|cs| {
        let mut groups = EVENT_GROUPS.borrow_ref_mut(cs);
        let Some(slot) = groups.iter_mut().find(|slot| **slot == 0) else {
            return false;
        };
        *slot = group as usize;
        true
    })
}

fn unregister_event_group(group: *mut EventGroup) {
    critical_section::with(|cs| {
        let mut groups = EVENT_GROUPS.borrow_ref_mut(cs);
        if let Some(slot) = groups.iter_mut().find(|slot| **slot == group as usize) {
            *slot = 0;
        }
    });
}

/// Copies live event-group snapshots without changing their state.
pub fn event_diagnostics(output: &mut [EventDiagnostic]) -> usize {
    critical_section::with(|cs| {
        let groups = EVENT_GROUPS.borrow_ref(cs);
        let mut count = 0;
        for pointer in groups.iter().copied().filter(|pointer| *pointer != 0) {
            if count == output.len() {
                break;
            }
            // SAFETY: live event groups stay registered from successful init
            // until destroy, and all fields are serialized by this same
            // single-hart critical section.
            let group = unsafe { &*(pointer as *const EventGroup) };
            output[count] = EventDiagnostic {
                event: pointer,
                bits: group.bits,
                reads: group.reads,
                writes: group.writes,
                matches: group.matches,
                last_read_mask: group.last_read_mask,
                last_write_mask: group.last_write_mask,
                last_mode: group.last_mode,
            };
            count += 1;
        }
        count
    })
}

/// Mirrors C `osal_event { void *event; }`.
#[repr(C)]
pub struct OsalEvent {
    event: *mut c_void,
}

/// Create an event group.
#[unsafe(no_mangle)]
pub extern "C" fn osal_event_init(event_obj: *mut OsalEvent) -> c_int {
    if event_obj.is_null() {
        return OSAL_NOK;
    }
    let g = crate::alloc::osal_kmalloc(core::mem::size_of::<EventGroup>()) as *mut EventGroup;
    if g.is_null() {
        return OSAL_NOK;
    }
    // SAFETY: freshly allocated.
    unsafe {
        g.write(EventGroup {
            bits: 0,
            waiters: [EventWaiter::EMPTY; MAX_EVENT_WAITERS],
            reads: 0,
            writes: 0,
            matches: 0,
            last_read_mask: 0,
            last_write_mask: 0,
            last_mode: 0,
        });
        (*event_obj).event = g as *mut c_void;
    }
    if !register_event_group(g) {
        crate::alloc::osal_kfree(g as *mut c_void);
        unsafe { (*event_obj).event = core::ptr::null_mut() };
        return OSAL_NOK;
    }
    OSAL_OK
}

fn event_ptr(event_obj: *mut OsalEvent) -> *mut EventGroup {
    if event_obj.is_null() {
        return core::ptr::null_mut();
    }
    unsafe { (*event_obj).event as *mut EventGroup }
}

fn event_matches(bits: u32, mask: u32, mode: u32) -> Option<u32> {
    let matched = bits & mask;
    let satisfied = if mode & WAITMODE_AND != 0 {
        matched == mask && mask != 0
    } else {
        matched != 0
    };
    satisfied.then_some(matched)
}

fn poll_event(group: &mut EventGroup, mask: u32, mode: u32) -> Option<u32> {
    let matched = event_matches(group.bits, mask, mode)?;
    group.matches = group.matches.saturating_add(1);
    if mode & WAITMODE_CLR != 0 {
        group.bits &= !matched;
    }
    Some(matched)
}

fn unregister_waiter(group: *mut EventGroup, semaphore: usize) {
    critical_section::with(|_cs| {
        let group = unsafe { &mut *group };
        if let Some(waiter) = group
            .waiters
            .iter_mut()
            .find(|waiter| waiter.semaphore == semaphore)
        {
            *waiter = EventWaiter::EMPTY;
        }
    });
}

/// Wait up to `timeout_ms` (`u32::MAX` == forever) for `mask` bits (OR = any,
/// AND = all; CLR clears them on success). Returns the matched bits, or 0 on
/// timeout. Each blocked reader registers its mask/mode and a private semaphore;
/// a write wakes every matching reader, matching LiteOS `LOS_EventWrite`.
#[unsafe(no_mangle)]
pub extern "C" fn osal_event_read(
    event_obj: *mut OsalEvent,
    mask: c_uint,
    timeout_ms: c_uint,
    mode: c_uint,
) -> c_int {
    let g = event_ptr(event_obj);
    if g.is_null() {
        return 0;
    }
    let forever = timeout_ms == u32::MAX;
    let deadline = crate::osal_ext::osal_get_jiffies().wrapping_add(timeout_ms as u64);
    loop {
        let matched = critical_section::with(|_cs| {
            let e = unsafe { &mut *g };
            e.reads = e.reads.saturating_add(1);
            e.last_read_mask = mask;
            e.last_mode = mode;
            poll_event(e, mask, mode)
        });
        if let Some(m) = matched {
            return m as c_int;
        }
        let remaining = if forever {
            u32::MAX
        } else {
            let now = crate::osal_ext::osal_get_jiffies();
            if now >= deadline {
                return 0;
            }
            (deadline - now).min(u32::MAX as u64) as u32
        };
        let semaphore = core::pin::pin!(Semaphore::new(0));
        if semaphore.try_init().is_err() {
            return OSAL_NOK;
        }
        let semaphore_pointer = semaphore.as_ref().get_ref() as *const Semaphore as usize;
        let registration = critical_section::with(|_cs| {
            let e = unsafe { &mut *g };
            if let Some(matched) = poll_event(e, mask, mode) {
                return Ok(Some(matched));
            }
            let Some(waiter) = e.waiters.iter_mut().find(|waiter| waiter.semaphore == 0) else {
                return Err(());
            };
            *waiter = EventWaiter {
                semaphore: semaphore_pointer,
                mask,
                mode,
            };
            Ok(None)
        });

        match registration {
            Ok(Some(matched)) => {
                let _ = unsafe { semaphore.destroy() };
                return matched as c_int;
            }
            Err(()) => {
                let _ = unsafe { semaphore.destroy() };
                return OSAL_NOK;
            }
            Ok(None) => {}
        }

        let wait = semaphore.down_timeout(WaitTimeout::from_millis(remaining));
        unregister_waiter(g, semaphore_pointer);
        let _ = unsafe { semaphore.destroy() };
        if !matches!(wait, Ok(WaitOutcome::Acquired)) {
            return OSAL_NOK;
        }
    }
}

/// Set `mask` bits and wake every waiter whose condition is satisfied.
#[unsafe(no_mangle)]
pub extern "C" fn osal_event_write(event_obj: *mut OsalEvent, mask: c_uint) -> c_int {
    let g = event_ptr(event_obj);
    if g.is_null() {
        return OSAL_NOK;
    }
    let waiters = critical_section::with(|_cs| {
        let e = unsafe { &mut *g };
        e.bits |= mask;
        e.writes = e.writes.saturating_add(1);
        e.last_write_mask = mask;
        let mut waiters = [0; MAX_EVENT_WAITERS];
        for (output, waiter) in waiters.iter_mut().zip(e.waiters.iter()) {
            if waiter.semaphore != 0 && event_matches(e.bits, waiter.mask, waiter.mode).is_some() {
                *output = waiter.semaphore;
            }
        }
        waiters
    });
    for pointer in waiters.into_iter().filter(|pointer| *pointer != 0) {
        let semaphore = unsafe { &*(pointer as *const Semaphore) };
        if semaphore.up().is_err() {
            return OSAL_NOK;
        }
    }
    OSAL_OK
}

/// Clear `mask` bits.
#[unsafe(no_mangle)]
pub extern "C" fn osal_event_clear(event_obj: *mut OsalEvent, mask: c_uint) -> c_int {
    let g = event_ptr(event_obj);
    if g.is_null() {
        return OSAL_NOK;
    }
    critical_section::with(|_cs| {
        let e = unsafe { &mut *g };
        e.bits &= !mask;
    });
    OSAL_OK
}

/// Destroy an event group.
#[unsafe(no_mangle)]
pub extern "C" fn osal_event_destroy(event_obj: *mut OsalEvent) -> c_int {
    let g = event_ptr(event_obj);
    if g.is_null() {
        return OSAL_NOK;
    }
    // Event destruction requires no active reader/writer.
    debug_assert!(unsafe { (*g).waiters.iter().all(|waiter| waiter.semaphore == 0) });
    unregister_event_group(g);
    crate::alloc::osal_kfree(g as *mut c_void);
    if !event_obj.is_null() {
        unsafe { (*event_obj).event = core::ptr::null_mut() };
    }
    OSAL_OK
}
