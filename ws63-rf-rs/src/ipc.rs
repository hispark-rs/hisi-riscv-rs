//! Framework + host↔device IPC contract (ws63-RF `port_frw.h` / `port_hcc.h`).
//!
//! These are typed STUBS for the parts of the WS63 message-routing framework
//! (FRW) and host↔device-MAC transport (HCC) that the vendor blobs reference
//! but do **not** themselves define — the porting layer must supply them. On
//! the single-core WS63 the "host↔device" link is a shared-memory IPC, not a
//! real coprocessor link. A working implementation is the heart of phase 4
//! (the FRW worker thread + timers over `crate::sched`, the shared-memory
//! queue/ring transport). Until then each initialiser returns `OSAL_NOK` (fail
//! fast — no silent fake success) and allocators return null.
//!
//! The FRW/WLAN entry points the blob defines itself (`frw_main_init`,
//! `frw_send_msg_to_device`, `wlan_*_abb_rf_clk`, `wlan_msg_h2d_*`, …) are
//! intentionally NOT defined here — the vendor `.a` libraries own them; defining
//! our own would be a duplicate-symbol conflict at link time.

use crate::{OSAL_NOK, OSAL_OK};
use core::ffi::{c_int, c_void};

type MsgHook = Option<unsafe extern "C" fn(*mut c_void)>;

// ── FRW: framework / message dispatch / timers (porting-supplied parts) ──────

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
/// STUB: no worker thread yet (would run on `crate::sched`).
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

// ── HCC: host↔device transport (porting-supplied) ───────────────────────────

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
