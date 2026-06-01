//! Framework + host↔device IPC contract (ws63-RF `port_frw.h` / `port_hcc.h` /
//! `port_wlan.h`).
//!
//! These are all typed STUBS. They are the WS63 message-routing framework (FRW),
//! the host↔device-MAC transport (HCC — on the single-core WS63 a shared-memory
//! IPC, not a real coprocessor link), and the host→device descriptor rings /
//! RF-clock control (WLAN). Implementing them for real is the heart of phase 4
//! and needs: a task scheduler (the FRW worker thread + timers), the
//! shared-memory queue/ring transport, and the vendor RF/MAC HAL. Until then
//! every initialiser returns `OSAL_NOK` (fail fast — no silent fake success) and
//! allocators return null.

use crate::{OSAL_NOK, OSAL_OK};
use core::ffi::{c_int, c_void};

type MsgHook = Option<unsafe extern "C" fn(*mut c_void)>;

// ── FRW: framework / message dispatch / timers ──────────────────────────────

/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn frw_main_init() -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn frw_main_exit() -> c_int {
    OSAL_OK
}
/// STUB: framework never reaches the ready state.
#[unsafe(no_mangle)]
pub extern "C" fn frw_get_init_state() -> c_int {
    0
}
/// STUB: no message pool.
#[unsafe(no_mangle)]
pub extern "C" fn frw_fetch_msg_node() -> *mut c_void {
    core::ptr::null_mut()
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn frw_free_msg_node(_msg: *mut c_void) {}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn frw_send_msg_to_device(_msg: *mut c_void) -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn frw_dmac_msg_hook_register(_hook: MsgHook) -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn frw_dmac_msg_hook_unregister(_hook: MsgHook) -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn frw_dmac_rcv_cfg(_cfg: *mut c_void) -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn frw_rx_netbuf(_netbuf: *mut c_void) -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn frw_rx_wifi_post_netbuf(_netbuf: *mut c_void) -> c_int {
    OSAL_NOK
}
/// STUB.
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
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn frw_event_process_all_event_etc() {}
/// STUB: no worker thread (needs a scheduler).
#[unsafe(no_mangle)]
pub extern "C" fn frw_task_thread(_arg: *mut c_void) -> *mut c_void {
    core::ptr::null_mut()
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn frw_thread_get_wait() -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn frw_rom_cb_register(_cb: *mut c_void) -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn frw_hcc_service_deinit() -> c_int {
    OSAL_OK
}

// ── HCC: host↔device transport ──────────────────────────────────────────────

/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn hcc_dmac_config_bus_ini() -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn hcc_dmac_service_adapt_start() -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn hcc_msg_open_wlan() -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn hcc_msg_close_wlan() -> c_int {
    OSAL_OK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn hcc_wifi_msg_register(_handler: MsgHook) -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn hcc_wifi_msg_send(_msg: *mut c_void) -> c_int {
    OSAL_NOK
}

// ── WLAN: descriptor rings + RF clock ───────────────────────────────────────

/// Lock-free ring control (ws63-RF `port_wlan.h` `oal_ring_ctrl`).
#[repr(C)]
pub struct OalRingCtrl {
    /// Base address of the ring buffer in shared memory.
    pub entries_addr: *mut c_void,
    /// Address of the read index (7-bit idx + 1-bit wrap).
    pub read_idx_addr: *mut u8,
    /// Address of the write index.
    pub write_idx_addr: *mut u8,
    /// Total number of entries.
    pub ring_depth: u16,
    /// Size of each entry in bytes.
    pub ring_entry_size: u16,
}

/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn wlan_open_wifi_abb_rf_clk() -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn wlan_close_wifi_abb_rf_clk() -> c_int {
    OSAL_OK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn wlan_msg_h2d_init_rx_dscr() -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn wlan_msg_h2d_destory_rx_dscr() -> c_int {
    OSAL_OK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn wlan_msg_h2d_enable_front_end() -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn wlan_msg_h2d_idel_sleep_prepare() -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn wlan_msg_h2d_req_sync_pmbit() -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn wlan_msg_h2d_sync_sub_work_to_rf() -> c_int {
    OSAL_NOK
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn wlan_msg_h2d_sync_work_vap_state() -> c_int {
    OSAL_NOK
}
/// STUB: ring is always empty/full.
#[unsafe(no_mangle)]
pub extern "C" fn oal_ring_write(_ring: *mut OalRingCtrl, _entry: *const c_void) -> c_int {
    -1
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn oal_ring_read(_ring: *mut OalRingCtrl, _entry: *mut c_void) -> c_int {
    -1
}
/// STUB.
#[unsafe(no_mangle)]
pub extern "C" fn oal_get_ring_element_num(_ring: *mut OalRingCtrl) -> u16 {
    0
}
