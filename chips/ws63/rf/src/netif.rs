//! netif / lwip boundary — the seam between the vendor MAC blob and a TCP/IP
//! stack.
//!
//! The WS63 WiFi driver was built against lwip 2.1.3: on TX it asks for packet
//! buffers via [`pbuf_alloc`] and hands frames down; on RX it pushes received
//! frames up via [`driverif_input`]; interfaces are managed through the
//! `netifapi_*` calls. The north-star plan replaces C lwip with **smoltcp**, so
//! these are the integration points where Rust takes over.
//!
//! ## STATUS
//!
//! - `pbuf_*` use the exact WS63 app layout verified by
//!   `tools/check-pbuf-layout.sh` against the SDK headers.
//! - `driverif_input` queues bounded frames when `net` is enabled.
//! - [`transmit`] calls the vendor-installed `netif.drv_send` callback.
//! - `netifapi_*` / `tcpip_callback` are accepted no-ops (no TCP/IP thread yet).
//!
//! ## pbuf layout boundary
//!
//! `struct pbuf` and `struct netif` are heavily `#if`-configured. The constants
//! below are for the delivered WS63 `ws63-liteos-app` archives and are checked
//! with the original cross compiler and headers, rather than inferred from
//! upstream lwIP defaults.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::{c_int, c_void};
use portable_atomic::{AtomicU32, AtomicUsize, Ordering};

const NETIF_DRV_SEND_OFFSET: usize = 244;
const NETIF_HWADDR_OFFSET: usize = 268;
const NETIF_HWADDR_LEN_OFFSET: usize = 274;
const PBUF_TYPE_RAM: u8 = 0x80;
const NETIF_NO_INDEX: u8 = 0;
const PBUF_ZERO_COPY_TAILROOM: usize = 4;
// The delivered lwIP configuration sets ETH_PAD_SIZE=2. The SDK RX adapter
// exposes those two alignment bytes before calling `driverif_input`; smoltcp's
// Ethernet device contract starts at the destination MAC and must not see them.
#[cfg(feature = "net")]
const ETH_PAD_SIZE: usize = 2;

/// Frames handed up by [`driverif_input`] and dropped (until smoltcp is wired).
static RX_DROPPED: AtomicU32 = AtomicU32::new(0);
static RX_RECEIVED: AtomicU32 = AtomicU32::new(0);
static TX_FAILED: AtomicU32 = AtomicU32::new(0);
/// Single STA netif registered by the vendor WAL. Scan bring-up has no TCP/IP
/// stack yet, but WAL still expects lwIP to preserve this opaque identity.
static REGISTERED_NETIF: AtomicUsize = AtomicUsize::new(0);

/// Number of RX frames dropped at the netif seam so far (diagnostic).
pub fn rx_dropped() -> u32 {
    RX_DROPPED.load(Ordering::Relaxed)
}

/// Number of valid Ethernet frames handed up by the vendor driver.
pub fn rx_received() -> u32 {
    RX_RECEIVED.load(Ordering::Relaxed)
}

/// Frames that could not be handed to the vendor TX callback.
pub fn tx_failed() -> u32 {
    TX_FAILED.load(Ordering::Relaxed)
}

/// Read-only snapshot of the vendor-created lwIP interface.
///
/// This is a bring-up aid for checking the Rust ABI offsets against the exact
/// SDK object at runtime. No function pointer is invoked while collecting it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetifDiagnostics {
    /// Address passed by the vendor WAL to `netifapi_netif_add`.
    pub address: usize,
    /// Raw `netif.drv_send` function address.
    pub driver_send: usize,
    /// Raw `netif.hwaddr_len` value.
    pub hardware_address_len: u8,
    /// Raw six-byte `netif.hwaddr` storage.
    pub hardware_address: [u8; 6],
}

#[cfg(feature = "rf-queue-guard")]
pub(crate) fn frw_host_queue_base() -> usize {
    unsafe extern "C" {
        fn frw_netbuf_hook_register(kind: u16, callback: *const c_void) -> c_int;
    }
    let register = frw_netbuf_hook_register as *const u8;
    // SAFETY: see `netbuf_hook_diagnostics`; `g_frw_thread_ctrl` immediately
    // follows the 24-byte `g_netbuf_d2h_ctrl` object in frw_hmac/frw_thread.
    unsafe { core::ptr::read_unaligned(register.add(12).cast::<u32>()) as usize + 24 }
}

/// Arm trigger 0 on the first host FRW queue's immutable callback word.
#[cfg(all(feature = "rf-queue-guard", target_arch = "riscv32"))]
#[doc(hidden)]
pub fn arm_host_queue_callback_watchpoint() {
    let address = frw_host_queue_base() + 8 + 16;
    // mcontrol, machine mode, exact 32-bit store match, breakpoint exception.
    let control = 0x2003_0042_u32;
    unsafe {
        core::arch::asm!("csrw tselect, zero", options(nomem, nostack));
        core::arch::asm!("csrw tdata2, {address}", address = in(reg) address, options(nomem, nostack));
        core::arch::asm!("csrw tdata1, {control}", control = in(reg) control, options(nomem, nostack));
    }
}

/// Snapshot the registered vendor interface without calling into it.
pub fn diagnostics() -> Option<NetifDiagnostics> {
    let netif = REGISTERED_NETIF.load(Ordering::Acquire) as *const u8;
    if netif.is_null() {
        return None;
    }
    let mut hardware_address = [0; 6];
    // SAFETY: the interface remains vendor-owned for the firmware lifetime;
    // these offsets are checked by the SDK compiler oracle.
    let (driver_send, hardware_address_len) = unsafe {
        core::ptr::copy_nonoverlapping(
            netif.add(NETIF_HWADDR_OFFSET),
            hardware_address.as_mut_ptr(),
            hardware_address.len(),
        );
        (
            core::ptr::read_unaligned(netif.add(NETIF_DRV_SEND_OFFSET).cast::<usize>()),
            netif.add(NETIF_HWADDR_LEN_OFFSET).read(),
        )
    };
    Some(NetifDiagnostics {
        address: netif as usize,
        driver_send,
        hardware_address_len,
        hardware_address,
    })
}

/// WS63 app-build lwIP `struct pbuf`.
#[repr(C)]
struct Pbuf {
    next: *mut Pbuf,
    payload: *mut c_void,
    tot_len: u16,
    len: u16,
    list: *mut Pbuf,
    malloc_len: u16,
    type_internal: u8,
    _type_pad: u8,
    flags: u16,
    _flags_pad: u16,
    ref_count: i32,
    if_idx: u8,
    priority: u8,
    _tail_pad: [u8; 2],
    // packet bytes follow this header in the same allocation
}

const PBUF_HDR: usize = core::mem::size_of::<Pbuf>();
#[cfg(target_arch = "riscv32")]
const _: () = {
    assert!(PBUF_HDR == 32);
    assert!(core::mem::offset_of!(Pbuf, payload) == 4);
    assert!(core::mem::offset_of!(Pbuf, len) == 10);
    assert!(core::mem::offset_of!(Pbuf, malloc_len) == 16);
    assert!(core::mem::offset_of!(Pbuf, type_internal) == 18);
    assert!(core::mem::offset_of!(Pbuf, flags) == 20);
    assert!(core::mem::offset_of!(Pbuf, ref_count) == 24);
    assert!(core::mem::offset_of!(Pbuf, if_idx) == 28);
    assert!(core::mem::offset_of!(Pbuf, priority) == 29);
};
// The WS63 LiteOS lwIP configuration sets PBUF_ZERO_COPY_RESERVE to 80.
// `oal_pbuf_netbuf_alloc` exposes this area as the netbuf's HCC/FRW/MAC
// headroom by setting `data = pbuf->payload - 0x50`.
const PBUF_ZERO_COPY_RESERVE: usize = 80;

/// `pbuf_alloc(layer, length, type)` — allocate a single (unchained) pbuf with
/// the WS63 zero-copy headroom between its header and payload.
#[unsafe(no_mangle)]
pub extern "C" fn pbuf_alloc(_layer: c_int, length: u16, _type: c_int) -> *mut c_void {
    let total = PBUF_HDR + PBUF_ZERO_COPY_RESERVE + length as usize + PBUF_ZERO_COPY_TAILROOM;
    let raw = crate::alloc::osal_kmalloc(total) as *mut Pbuf;
    if raw.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: freshly allocated `total` bytes.
    unsafe {
        (*raw).next = core::ptr::null_mut();
        (*raw).payload = (raw as *mut u8).add(PBUF_HDR + PBUF_ZERO_COPY_RESERVE) as *mut c_void;
        (*raw).tot_len = length;
        (*raw).len = length;
        (*raw).list = core::ptr::null_mut();
        (*raw).malloc_len = total as u16;
        (*raw).type_internal = PBUF_TYPE_RAM;
        (*raw)._type_pad = 0;
        (*raw).flags = 0;
        (*raw)._flags_pad = 0;
        (*raw).ref_count = 1;
        (*raw).if_idx = NETIF_NO_INDEX;
        (*raw).priority = 0;
        (*raw)._tail_pad = [0; 2];
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
    let free = critical_section::with(|_| {
        // SAFETY: `p` came from pbuf_alloc and the short critical section
        // serializes task/IRQ reference-count updates on this single hart.
        unsafe {
            if (*p).ref_count > 1 {
                (*p).ref_count -= 1;
                false
            } else {
                (*p).ref_count = 0;
                true
            }
        }
    });
    if !free {
        return 0;
    }
    crate::alloc::osal_kfree(p as *mut c_void);
    1
}

/// `pbuf_ref(p)` — take an extra reference.
#[unsafe(no_mangle)]
pub extern "C" fn pbuf_ref(p: *mut c_void) {
    let p = p as *mut Pbuf;
    if !p.is_null() {
        critical_section::with(|_| {
            // SAFETY: valid pbuf; this short update is serialized with free.
            unsafe { (*p).ref_count += 1 };
        });
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
        let old_len = (*p).len as isize;
        let new_len = old_len + header_size as isize;
        let new_tot_len = (*p).tot_len as isize + header_size as isize;
        let new_payload = ((*p).payload as isize) - header_size as isize;
        let base = (p as *mut u8).add(PBUF_HDR) as isize;
        let end = p as isize + (*p).malloc_len as isize;
        if new_len < 0
            || new_len > u16::MAX as isize
            || new_tot_len < 0
            || new_tot_len > u16::MAX as isize
            || new_payload < base
            || new_payload + new_len > end
        {
            return 1;
        }
        (*p).payload = new_payload as *mut c_void;
        (*p).len = new_len as u16;
        (*p).tot_len = new_tot_len as u16;
    }
    0
}

/// Failure to hand a frame to the vendor data path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxError {
    /// The vendor station netif has not been registered yet.
    NoInterface,
    /// The registered netif has no driver send callback.
    NoDriver,
    /// The frame is empty or exceeds the pbuf length contract.
    InvalidLength,
    /// Packet allocation failed.
    NoMemory,
}

/// Send one complete Ethernet frame through the vendor-installed station
/// `netif.drv_send` callback.
pub fn transmit(frame: &[u8]) -> Result<(), TxError> {
    if frame.is_empty() || frame.len() > u16::MAX as usize {
        return Err(TxError::InvalidLength);
    }
    let netif = REGISTERED_NETIF.load(Ordering::Acquire) as *mut u8;
    if netif.is_null() {
        return Err(TxError::NoInterface);
    }
    type DriverSend = unsafe extern "C" fn(*mut c_void, *mut c_void);
    // SAFETY: the offset is verified against the exact SDK config by
    // `tools/check-pbuf-layout.sh`; the registered object remains vendor-owned.
    let send = unsafe {
        core::ptr::read_unaligned(
            netif
                .add(NETIF_DRV_SEND_OFFSET)
                .cast::<Option<DriverSend>>(),
        )
    }
    .ok_or(TxError::NoDriver)?;

    let pbuf = pbuf_alloc(0, frame.len() as u16, 0) as *mut Pbuf;
    if pbuf.is_null() {
        return Err(TxError::NoMemory);
    }
    // SAFETY: pbuf_alloc created a writable payload of exactly frame.len().
    unsafe {
        core::ptr::copy_nonoverlapping(frame.as_ptr(), (*pbuf).payload.cast(), frame.len());
        send(netif.cast(), pbuf.cast());
    }
    // `drv_send` takes its own asynchronous reference. Match lwIP's caller
    // ownership by releasing the reference created by pbuf_alloc.
    pbuf_free(pbuf.cast());
    Ok(())
}

#[cfg(feature = "net")]
#[cfg_attr(not(target_arch = "riscv32"), allow(dead_code))]
pub(crate) fn vendor_tx_sink(frame: &[u8]) {
    if transmit(frame).is_err() {
        TX_FAILED.fetch_add(1, Ordering::Relaxed);
    }
}

/// MAC address installed on the sole vendor station netif.
pub fn hardware_address() -> Option<[u8; 6]> {
    diagnostics().and_then(|snapshot| {
        (snapshot.hardware_address_len == 6).then_some(snapshot.hardware_address)
    })
}

/// `driverif_input(netif, p)` — RX entry from the MAC driver. With feature `net`
/// the frame bytes are pushed to the smoltcp bridge (`crate::netif_smoltcp`);
/// otherwise (or if the pbuf has no payload) the frame is counted and dropped.
/// The pbuf is freed either way (lwip owns it after input).
#[unsafe(no_mangle)]
pub extern "C" fn driverif_input(_netif: *mut c_void, p: *mut c_void) -> c_int {
    #[cfg(feature = "net")]
    {
        let pb = p as *const Pbuf;
        if !pb.is_null() {
            // SAFETY: `p` is a live pbuf from pbuf_alloc; payload/len are set by
            // the driver. Copy the single-buffer frame to the smoltcp RX queue.
            let (payload, len) = unsafe { ((*pb).payload, (*pb).len as usize) };
            if !payload.is_null()
                && len > ETH_PAD_SIZE
                && len - ETH_PAD_SIZE <= crate::netif_smoltcp::MTU
            {
                let bytes = unsafe {
                    core::slice::from_raw_parts(
                        payload.cast::<u8>().add(ETH_PAD_SIZE),
                        len - ETH_PAD_SIZE,
                    )
                };
                crate::netif_smoltcp::rx_push(bytes);
                RX_RECEIVED.fetch_add(1, Ordering::Relaxed);
            } else {
                RX_DROPPED.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    #[cfg(not(feature = "net"))]
    {
        RX_DROPPED.fetch_add(1, Ordering::Relaxed);
    }
    pbuf_free(p);
    0
}

// ── Interface management / tcpip thread ─────────────────────────────────────
// Scan only needs one opaque STA netif identity. These functions preserve that
// control-plane contract without claiming that an IP data plane exists.

/// `netifapi_netif_add` — register the vendor-created STA interface.
#[unsafe(no_mangle)]
pub extern "C" fn netifapi_netif_add(
    netif: *mut c_void,
    _ipaddr: *const u32,
    _netmask: *const u32,
    _gateway: *const u32,
) -> c_int {
    REGISTERED_NETIF.store(netif as usize, Ordering::Release);
    0
}

/// `netifapi_netif_remove` — deregister the STA interface.
#[unsafe(no_mangle)]
pub extern "C" fn netifapi_netif_remove(netif: *mut c_void) -> c_int {
    let _ =
        REGISTERED_NETIF.compare_exchange(netif as usize, 0, Ordering::AcqRel, Ordering::Acquire);
    0
}

/// `netifapi_netif_find_by_name` — return the sole registered STA netif.
///
/// The scan milestone supports exactly one interface, so name disambiguation
/// is intentionally deferred until the data-plane netif implementation.
#[unsafe(no_mangle)]
pub extern "C" fn netifapi_netif_find_by_name(_name: *const u8) -> *mut c_void {
    REGISTERED_NETIF.load(Ordering::Acquire) as *mut c_void
}

/// `netifapi_netif_get_addr` — scan has no IPv4 configuration; report zeros.
#[unsafe(no_mangle)]
pub extern "C" fn netifapi_netif_get_addr(
    netif: *mut c_void,
    ipaddr: *mut u32,
    netmask: *mut u32,
    gateway: *mut u32,
) -> c_int {
    if netif.is_null() {
        return -6; // lwIP ERR_VAL
    }
    for output in [ipaddr, netmask, gateway] {
        if !output.is_null() {
            // SAFETY: lwIP supplies writable `ip4_addr_t` outputs.
            unsafe { output.write(0) };
        }
    }
    0
}

/// Register a lwIP extended-status callback. The scan-only adapter has no
/// tcpip thread to dispatch it, so retain no callback and report success.
#[unsafe(no_mangle)]
pub extern "C" fn netifapi_netif_add_ext_callback(
    _callback: *mut c_void,
    _function: *mut c_void,
) -> c_int {
    0
}

/// Disable IPv6 autoconfiguration for the opaque scan netif.
#[unsafe(no_mangle)]
pub extern "C" fn netifapi_set_ip6_autoconfig_disabled(_netif: *mut c_void) -> c_int {
    0
}

/// Accept creation of a link-local address for the opaque scan netif.
#[unsafe(no_mangle)]
pub extern "C" fn netifapi_netif_add_ip6_linklocal_address(
    _netif: *mut c_void,
    _from_mac_48bit: u8,
) -> c_int {
    0
}

/// Mark the opaque scan netif administratively up.
#[unsafe(no_mangle)]
pub extern "C" fn netifapi_netif_set_up(_netif: *mut c_void) -> c_int {
    0
}

/// Mark the opaque scan netif administratively down.
#[unsafe(no_mangle)]
pub extern "C" fn netifapi_netif_set_down(_netif: *mut c_void) -> c_int {
    0
}

/// Mark the opaque scan netif's link up.
#[unsafe(no_mangle)]
pub extern "C" fn netifapi_netif_set_link_up(_netif: *mut c_void) -> c_int {
    0
}

/// Select the opaque scan netif as the default route.
#[unsafe(no_mangle)]
pub extern "C" fn netifapi_netif_set_default(_netif: *mut c_void) -> c_int {
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
