//! Scheduler-backed OSAL message queues + event groups.
//!
//! C SDK `osal_msg_queue.h` / `osal_event.h` (deeper OSAL, beyond
//! `ws63-RF/port_*.h`). A queue is a heap ring buffer + a counting
//! `Semaphore` for blocking reads; an event group is a bitmask + a semaphore
//! the reader rechecks. Handles: the queue id is the heap object address; the
//! event handle field holds it.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::sched::Semaphore;
use crate::{OSAL_NOK, OSAL_OK};
use core::ffi::{c_char, c_int, c_uint, c_ulong, c_void};

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
        unsafe { (*q).items.up() };
        OSAL_OK
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
    if !unsafe { (*q).items.down_timeout(timeout) } {
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
    }
    crate::alloc::osal_kfree(q as *mut c_void);
}

// ── Event group (bitmask + semaphore the reader rechecks) ───────────────────

const WAITMODE_AND: c_uint = 4; // all bits (OR / any is the default else-branch)
const WAITMODE_CLR: c_uint = 1; // clear matched bits on success

struct EventGroup {
    bits: u32,
    sem: Semaphore,
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
            sem: Semaphore::new(0),
        });
        (*event_obj).event = g as *mut c_void;
    }
    OSAL_OK
}

fn event_ptr(event_obj: *mut OsalEvent) -> *mut EventGroup {
    if event_obj.is_null() {
        return core::ptr::null_mut();
    }
    unsafe { (*event_obj).event as *mut EventGroup }
}

/// Wait up to `timeout_ms` (`u32::MAX` == forever) for `mask` bits (OR = any,
/// AND = all; CLR clears them on success). Returns the matched bits, or 0 on
/// timeout. NOTE: single-waiter (the WiFi worker); a write wakes the waiter,
/// which rechecks.
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
            let m = e.bits & mask;
            let sat = if mode & WAITMODE_AND != 0 {
                m == mask && mask != 0
            } else {
                m != 0 // OR (default)
            };
            if sat {
                if mode & WAITMODE_CLR != 0 {
                    e.bits &= !m;
                }
                Some(m)
            } else {
                None
            }
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
        // SAFETY: g is a live handle. Block until a write() signals (or the
        // deadline passes), then recheck the bits at the top of the loop.
        unsafe { (*g).sem.down_timeout(remaining) };
    }
}

/// Set `mask` bits and wake a waiter.
#[unsafe(no_mangle)]
pub extern "C" fn osal_event_write(event_obj: *mut OsalEvent, mask: c_uint) -> c_int {
    let g = event_ptr(event_obj);
    if g.is_null() {
        return OSAL_NOK;
    }
    critical_section::with(|_cs| {
        let e = unsafe { &mut *g };
        e.bits |= mask;
    });
    // SAFETY: g is a live handle.
    unsafe { (*g).sem.up() };
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
    crate::alloc::osal_kfree(g as *mut c_void);
    if !event_obj.is_null() {
        unsafe { (*event_obj).event = core::ptr::null_mut() };
    }
    OSAL_OK
}
