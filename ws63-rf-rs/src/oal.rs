//! OAL Wi-Fi packet-buffer pool (ws63-RF `port_oal.h`).
//!
//! A simple bump reservation inside the 48 KB Wi-Fi packet RAM delimited by the
//! linker symbols `__wifi_pkt_ram_begin__ .. __wifi_pkt_ram_end__` (supplied by
//! build.rs). This is enough for `oal_mem_rsv` static reservations and the
//! pool-size bookkeeping the blob queries; the full netbuf sub-pool carving the
//! C SDK does (TXBFEE/PROTECT/COEX/BEACON/NETBUF) is a phase-4 TODO.

use core::cell::Cell;
use core::ffi::c_void;
use core::sync::atomic::{AtomicUsize, Ordering};
use critical_section::Mutex;

unsafe extern "C" {
    static __wifi_pkt_ram_begin__: u8;
    static __wifi_pkt_ram_end__: u8;
}

fn pkt_ram_begin() -> usize {
    (&raw const __wifi_pkt_ram_begin__) as usize
}
fn pkt_ram_end() -> usize {
    (&raw const __wifi_pkt_ram_end__) as usize
}

/// Bump cursor as an offset from `__wifi_pkt_ram_begin__`.
static CURSOR: Mutex<Cell<usize>> = Mutex::new(Cell::new(0));
static BUF_SIZE: AtomicUsize = AtomicUsize::new(0);
static SKB_SIZE: AtomicUsize = AtomicUsize::new(0);

/// Zero-copy netbuf header size used to align payloads. Scaffold constant; the
/// C SDK derives this from the netbuf layout (phase-4 TODO).
const ZEROCOPY_HDR_SIZE: usize = 64;

/// Get the zero-copy header size.
#[unsafe(no_mangle)]
pub extern "C" fn oal_mem_get_zerocopy_hdr_size() -> usize {
    ZEROCOPY_HDR_SIZE
}

/// Reserve `size` bytes (8-aligned) from the Wi-Fi packet RAM. Null on overflow.
#[unsafe(no_mangle)]
pub extern "C" fn oal_mem_rsv(size: usize) -> *mut c_void {
    // Round up to 8 (saturating: a near-usize::MAX request can't fit anyway).
    let size = size.saturating_add(7) & !7;
    critical_section::with(|cs| {
        let cur = CURSOR.borrow(cs);
        let off = cur.get();
        let base = pkt_ram_begin();
        // Overflow-safe bounds: compute start and end with checked arithmetic so
        // a wrapping `base + off + size` can never spoof an in-range result.
        let start = match base.checked_add(off) {
            Some(s) => s,
            None => return core::ptr::null_mut(),
        };
        let end = match start.checked_add(size) {
            Some(e) => e,
            None => return core::ptr::null_mut(),
        };
        if end > pkt_ram_end() {
            return core::ptr::null_mut();
        }
        cur.set(off + size);
        start as *mut c_void
    })
}

/// Set the network-buffer pool total size (stored for `oal_memory_init`).
#[unsafe(no_mangle)]
pub extern "C" fn oal_mem_set_buf_size(size: usize) {
    BUF_SIZE.store(size, Ordering::Relaxed);
}

/// Set the skb (socket-buffer) size.
#[unsafe(no_mangle)]
pub extern "C" fn oal_mem_set_skb_size(size: usize) {
    SKB_SIZE.store(size, Ordering::Relaxed);
}

/// Initialise the OAL pool (resets the bump cursor). Returns `OSAL_OK`.
#[unsafe(no_mangle)]
pub extern "C" fn oal_memory_init() -> i32 {
    critical_section::with(|cs| CURSOR.borrow(cs).set(0));
    crate::OSAL_OK
}

/// Tear down the OAL pool.
#[unsafe(no_mangle)]
pub extern "C" fn oal_memory_exit() -> i32 {
    crate::OSAL_OK
}

/// Number of entries the netbuf pool can hold (pool bytes / buf size).
#[unsafe(no_mangle)]
pub extern "C" fn oal_get_netbuf_pool_len() -> i32 {
    let buf = BUF_SIZE.load(Ordering::Relaxed);
    if buf == 0 {
        return 0;
    }
    let total = pkt_ram_end().saturating_sub(pkt_ram_begin());
    (total / buf) as i32
}

// ── General OAL allocation (driver structures, not packet RAM) ───────────────
// The C SDK `oal_mem_alloc(pool_id, len, lock)` / `oal_mem_free(ptr, lock)`
// macros normally expand to `*_etc()` (file/line traced); the blob also
// references the bare names (verified by nm). Both back onto the general heap.

/// Allocate `len` bytes from the general heap (pool id / lock are advisory).
#[unsafe(no_mangle)]
pub extern "C" fn oal_mem_alloc(
    _pool_id: core::ffi::c_int,
    len: core::ffi::c_uint,
    _lock: core::ffi::c_uchar,
) -> *mut c_void {
    crate::alloc::osal_kmalloc(len as usize)
}

/// Free a block from [`oal_mem_alloc`].
#[unsafe(no_mangle)]
pub extern "C" fn oal_mem_free(ptr: *mut c_void, _lock: core::ffi::c_uchar) {
    crate::alloc::osal_kfree(ptr);
}
