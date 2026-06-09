//! HCC — host↔device message transport (porting-supplied).
//!
//! On the single-core WS63 the "host controller channel" is an in-memory queue
//! rather than a real coprocessor bus: [`hcc_wifi_msg_send`] hands a message to
//! the [`crate::frw`] worker's FIFO, which delivers it to the handler registered
//! via [`hcc_wifi_msg_register`]. The bus-init / channel-lifecycle calls are
//! accepted (there is no physical bus to bring up).

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::frw::{self, FrwMsg, FrwMsgNode, MsgHandler};
use crate::{OSAL_NOK, OSAL_OK};
use core::ffi::c_int;

/// Initialise the DMAC transport bus. No physical bus on a single core — OK.
#[unsafe(no_mangle)]
pub extern "C" fn hcc_dmac_config_bus_ini() -> c_int {
    OSAL_OK
}

/// Start the DMAC transport service. OK.
#[unsafe(no_mangle)]
pub extern "C" fn hcc_dmac_service_adapt_start() -> c_int {
    OSAL_OK
}

/// Open the WLAN message channel. OK.
#[unsafe(no_mangle)]
pub extern "C" fn hcc_msg_open_wlan() -> c_int {
    OSAL_OK
}

/// Close the WLAN message channel. OK.
#[unsafe(no_mangle)]
pub extern "C" fn hcc_msg_close_wlan() -> c_int {
    OSAL_OK
}

/// Register the device-side handler invoked for each delivered message.
#[unsafe(no_mangle)]
pub extern "C" fn hcc_wifi_msg_register(handler: Option<MsgHandler>) -> c_int {
    frw::set_device_handler(handler);
    OSAL_OK
}

/// Send a message host→device. The message must come from
/// [`frw_fetch_msg_node`](crate::frw::frw_fetch_msg_node) (its node is at the
/// same address, since `frw_msg` is at offset 0). Returns `OSAL_OK`, or
/// `OSAL_NOK` if the message is null or the worker FIFO is full.
#[unsafe(no_mangle)]
pub extern "C" fn hcc_wifi_msg_send(msg: *mut FrwMsg) -> c_int {
    if msg.is_null() {
        return OSAL_NOK;
    }
    frw::post(msg as *mut FrwMsgNode)
}
