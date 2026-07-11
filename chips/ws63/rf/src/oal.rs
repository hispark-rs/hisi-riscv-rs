//! OAL allocation hooks not supplied by the WS63 mask ROM.
//!
//! The packet-RAM pool itself is owned by the original mask-ROM OAL
//! implementation. In particular, `oal_memory_init(start, end, cfg, count)`,
//! `oal_mem_rsv`, netbuf allocation/free, and the netbuf accessors must resolve
//! to `ws63_acore_rom.lds`: the Wi-Fi closure passes the vendor sub-pool table
//! and the ROM carves the RX/TX descriptor buffers exactly as silicon expects.
//! This module only supplies the general heap hooks that the application OS is
//! required to implement.

use core::ffi::c_void;

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
