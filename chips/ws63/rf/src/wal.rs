//! Narrow WS63 WAL ioctl boundary shared by the native Wi-Fi adapters.
//!
//! Layouts and command values come from the delivered
//! `soc_wifi_driver_wpa_if.h`. Higher layers pass typed payloads but never
//! duplicate the vendor command envelope or call the legacy supplicant's
//! `drv_soc_ioctl_*` convenience wrappers.

use core::ffi::{c_int, c_void};

#[repr(C)]
struct IoctlCommand {
    command: u32,
    buffer: *mut c_void,
}

#[cfg(target_arch = "riscv32")]
unsafe extern "C" {
    fn drv_soc_hwal_wpa_ioctl(ifname: *mut i8, command: *const IoctlCommand) -> c_int;
}

/// Issue one synchronous command through the vendor WAL.
///
/// `ifname` must contain a NUL terminator. The pointed payload only needs to
/// remain valid for this call; the delivered WAL copies or consumes each
/// supported payload synchronously.
pub(crate) fn ioctl(ifname: &[u8], command: u32, buffer: *mut c_void) -> c_int {
    if !ifname.contains(&0) {
        return -1;
    }
    #[cfg(target_arch = "riscv32")]
    {
        let request = IoctlCommand { command, buffer };
        // SAFETY: `ifname` is NUL-terminated and the request plus its payload
        // remain live for the synchronous vendor call.
        unsafe { drv_soc_hwal_wpa_ioctl(ifname.as_ptr().cast_mut().cast(), &request) }
    }
    #[cfg(not(target_arch = "riscv32"))]
    {
        let _ = (command, buffer);
        -1
    }
}

const _: () = {
    assert!(core::mem::offset_of!(IoctlCommand, buffer) == core::mem::size_of::<usize>());
    assert!(core::mem::size_of::<IoctlCommand>() == 2 * core::mem::size_of::<usize>());
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_terminated_interface_name() {
        assert_eq!(ioctl(b"wlan0", 0, core::ptr::null_mut()), -1);
    }
}
