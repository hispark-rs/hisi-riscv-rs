//! netif / lwip boundary — the seam between the vendor MAC blob and a TCP/IP
//! stack.
//!
//! The WS63 WiFi driver was built against lwip 2.1.3: on TX it asks for packet
//! buffers via [`pbuf_alloc`] and hands frames down; on RX it pushes received
//! frames up via [`driverif_input`]; interfaces are managed through the
//! `netifapi_*` calls. The north-star plan replaces C lwip with **smoltcp**, so
//! these are the integration points where Rust takes over.
//!
//! ## STATUS: seam only — NOT a working data path
//!
//! - `pbuf_*` allocate/manage a buffer the blob fills, but see the layout
//!   warning below.
//! - `driverif_input` currently **drops** received frames (counts them); wiring
//!   them into smoltcp is the next step.
//! - `netifapi_*` / `tcpip_callback` are accepted no-ops (no TCP/IP thread yet).
//!
//! ## ⚠ pbuf layout caveat
//!
//! `struct pbuf` (lwip `pbuf.h`) is heavily `#if`-configured (`LWIP_RIPPLE`,
//! `MEM_MALLOC_DMA_ALIGN`, `LWIP_USE_L2_METRICS`, zero-copy, `LWIP_PBUF_
//! CUSTOM_DATA`, …). The blob accesses `payload`/`len`/`tot_len`/`next` at the
//! offsets *it* was compiled with. The `Pbuf` struct below uses the **default**
//! layout;
//! before any real frame flows on hardware these offsets MUST be reconciled
//! with the exact `lwipopts.h` the WiFi `.a` was built with (a mismatch would
//! silently corrupt memory). For the current link/seam goal the definitions
//! only need to exist.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::{c_int, c_void};
use portable_atomic::{AtomicU32, Ordering};

/// Frames handed up by [`driverif_input`] and dropped (until smoltcp is wired).
static RX_DROPPED: AtomicU32 = AtomicU32::new(0);

/// Number of RX frames dropped at the netif seam so far (diagnostic).
pub fn rx_dropped() -> u32 {
    RX_DROPPED.load(Ordering::Relaxed)
}

/// Default-layout lwip `struct pbuf` (see the module-level layout caveat).
#[repr(C)]
struct Pbuf {
    next: *mut Pbuf,
    payload: *mut c_void,
    tot_len: u16,
    len: u16,
    list: *mut Pbuf,
    type_internal: u8,
    _pad: u8,
    flags: u16,
    ref_count: u32,
    // packet bytes follow this header in the same allocation
}

const PBUF_HDR: usize = core::mem::size_of::<Pbuf>();

/// `pbuf_alloc(layer, length, type)` — allocate a single (unchained) pbuf whose
/// payload directly follows the header. `layer`/`type` are ignored.
#[unsafe(no_mangle)]
pub extern "C" fn pbuf_alloc(_layer: c_int, length: u16, _type: c_int) -> *mut c_void {
    let total = PBUF_HDR + length as usize;
    let raw = crate::alloc::osal_kmalloc(total) as *mut Pbuf;
    if raw.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: freshly allocated `total` bytes.
    unsafe {
        (*raw).next = core::ptr::null_mut();
        (*raw).payload = (raw as *mut u8).add(PBUF_HDR) as *mut c_void;
        (*raw).tot_len = length;
        (*raw).len = length;
        (*raw).list = core::ptr::null_mut();
        (*raw).type_internal = 0;
        (*raw)._pad = 0;
        (*raw).flags = 0;
        (*raw).ref_count = 1;
    }
    raw as *mut c_void
}

/// `pbuf_free(p)` — drop one reference; frees at zero. Returns the number of
/// pbufs freed (lwip semantics: 1 when this pbuf is released, else 0).
#[unsafe(no_mangle)]
pub extern "C" fn pbuf_free(p: *mut c_void) -> u8 {
    let p = p as *mut Pbuf;
    if p.is_null() {
        return 0;
    }
    // SAFETY: `p` came from pbuf_alloc. Single-hart cooperative: no atomic RMW
    // race within the critical section the blob holds.
    unsafe {
        if (*p).ref_count > 1 {
            (*p).ref_count -= 1;
            return 0;
        }
    }
    crate::alloc::osal_kfree(p as *mut c_void);
    1
}

/// `pbuf_ref(p)` — take an extra reference.
#[unsafe(no_mangle)]
pub extern "C" fn pbuf_ref(p: *mut c_void) {
    let p = p as *mut Pbuf;
    if !p.is_null() {
        // SAFETY: valid pbuf.
        unsafe { (*p).ref_count += 1 };
    }
}

/// `pbuf_header(p, header_size)` — move `payload` by `header_size` bytes
/// (positive = expose a header in front; negative = hide one) and adjust the
/// lengths. Returns 0 on success, 1 if it would move past the allocation.
#[unsafe(no_mangle)]
pub extern "C" fn pbuf_header(p: *mut c_void, header_size: i16) -> u8 {
    let p = p as *mut Pbuf;
    if p.is_null() {
        return 1;
    }
    // SAFETY: valid pbuf from pbuf_alloc.
    unsafe {
        let new_payload = ((*p).payload as isize) - header_size as isize;
        let base = (p as *mut u8).add(PBUF_HDR) as isize;
        // Must stay within [header end, header end + tot_len].
        if new_payload < base {
            return 1;
        }
        (*p).payload = new_payload as *mut c_void;
        (*p).len = (*p).len.wrapping_add(header_size as u16);
        (*p).tot_len = (*p).tot_len.wrapping_add(header_size as u16);
    }
    0
}

/// `driverif_input(netif, p)` — RX entry from the MAC driver. SEAM: the frame is
/// counted and freed (dropped). Wiring it into smoltcp is the next step.
#[unsafe(no_mangle)]
pub extern "C" fn driverif_input(_netif: *mut c_void, p: *mut c_void) {
    RX_DROPPED.fetch_add(1, Ordering::Relaxed);
    pbuf_free(p);
}

// ── Interface management / tcpip thread (accepted no-ops) ───────────────────
// ABI-safe stubs: the caller passes args in a0.. and cleans up; these read none
// and report success so init proceeds. A real netif/tcpip layer is future work.

/// `netifapi_netif_add` — register an interface. STUB: OK.
#[unsafe(no_mangle)]
pub extern "C" fn netifapi_netif_add() -> c_int {
    0
}
/// `netifapi_netif_remove` — deregister an interface. STUB: OK.
#[unsafe(no_mangle)]
pub extern "C" fn netifapi_netif_remove() -> c_int {
    0
}
/// `netif_set_link_up_interface` — link-up callback. STUB.
#[unsafe(no_mangle)]
pub extern "C" fn netif_set_link_up_interface(_arg: *mut c_void) {}
/// `netif_set_link_down_interface` — link-down callback. STUB.
#[unsafe(no_mangle)]
pub extern "C" fn netif_set_link_down_interface(_arg: *mut c_void) {}
/// `tcpip_callback` — schedule work on the TCP/IP thread. STUB: there is no
/// TCP/IP thread yet, so the callback is dropped (returns OK). This is a seam
/// for the future smoltcp worker.
#[unsafe(no_mangle)]
pub extern "C" fn tcpip_callback(_function: *mut c_void, _ctx: *mut c_void) -> c_int {
    0
}
