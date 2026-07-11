//! Local HCC transport used by the standalone Rust self-test.
//!
//! Production WS63 uses the mask-ROM's centralized FRW/HCC entry points. This
//! module deliberately does not export their C symbol names: doing so would
//! mask the ROM DMAC event loop and its `osal_wait *` ABI. The local queue below
//! only exercises the Rust scheduler and callback plumbing without the blob.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::frw::{self, FrwMsg, FrwMsgNode, MsgHandler};
use crate::{OSAL_NOK, OSAL_OK};
use core::ffi::c_int;

/// Register the local self-test handler.
pub(crate) fn register_local(handler: Option<MsgHandler>) -> c_int {
    frw::set_device_handler(handler);
    OSAL_OK
}

/// Send one local self-test message to the Rust worker.
pub(crate) fn send_local(msg: *mut FrwMsg) -> c_int {
    if msg.is_null() {
        return OSAL_NOK;
    }
    frw::post(msg as *mut FrwMsgNode)
}
