//! Narrow instrumentation for the guarded full Wi-Fi init image.

#[cfg(target_arch = "riscv32")]
unsafe extern "C" {
    fn __ws63_vendor_fe_rf_dev_set_ops_ext(cfg: u8);
    fn __ws63_vendor_hmac_main_init_later() -> i32;
    fn __real_hmac_main_init_etc() -> i32;
    fn __real_wal_main_init() -> i32;
    fn __real_wal_customize_set_config() -> u32;
    fn __ws63_vendor_wal_sync_post2hmac_no_rsp(
        vap_id: u8,
        msg_id: u16,
        data: *const u8,
        len: u16,
    ) -> u32;
    fn __ws63_vendor_wal_sync_send2device_no_rsp(
        vap_id: u8,
        msg_id: u16,
        data: *const u8,
        len: u16,
    ) -> u32;
    fn __ws63_vendor_frw_sync_host_post_msg(
        msg_id: u16,
        vap_id: u8,
        timeout_ms: u16,
        msg: *mut core::ffi::c_void,
    ) -> i32;
    fn __ws63_vendor_frw_send_cfg_to_device_sync(
        msg_id: u16,
        vap_id: u8,
        timeout_ms: u16,
        msg: *mut core::ffi::c_void,
    ) -> i32;
    fn __ws63_vendor_oal_get_netdev_by_name(name: *const u8) -> *mut core::ffi::c_void;
    fn __ws63_vendor_oal_net_register_netdev(netdev: *mut core::ffi::c_void) -> u32;
}

fn hex8(value: u32) -> [u8; 8] {
    let mut output = [0; 8];
    let mut index = 0;
    while index < output.len() {
        let nibble = (value >> ((7 - index) * 4)) & 0xf;
        output[index] = if nibble < 10 {
            b'0' + nibble as u8
        } else {
            b'a' + (nibble - 10) as u8
        };
        index += 1;
    }
    output
}

/// Trace the ROM caller and preserve the vendor callback behavior exactly.
#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fe_rf_dev_set_ops_ext(cfg: u8) {
    let caller: u32;
    // SAFETY: reads the current return-address register without touching memory.
    unsafe {
        core::arch::asm!("mv {caller}, ra", caller = out(reg) caller, options(nomem, nostack));
    }
    crate::log_emit(b"RFDBG_OPS cfg=0x");
    crate::log_emit(&hex8(cfg as u32));
    crate::log_emit(b" ra=0x");
    crate::log_emit(&hex8(caller));
    crate::log_emit(b"\r\n");

    // SAFETY: the build lane renames the exact vendor implementation and keeps
    // its original `void(u8)` ABI; this wrapper forwards the value unchanged.
    unsafe { __ws63_vendor_fe_rf_dev_set_ops_ext(cfg) };
}

fn trace_stage(name: &[u8], result: u32) {
    diag_emit(b"RFDBG_STAGE ");
    diag_emit(name);
    diag_emit(b"=0x");
    diag_emit(&hex8(result));
    diag_emit(b"\r\n");
}

fn diag_emit(bytes: &[u8]) {
    #[cfg(not(target_arch = "riscv32"))]
    {
        let _ = bytes;
        return;
    }

    #[cfg(target_arch = "riscv32")]
    {
        const DATA: *mut u16 = 0x4401_0004 as *mut u16;
        const ST: *const u16 = 0x4401_0044 as *const u16;
        const TX_FULL: u16 = 1 << 0;
        const TX_EMPTY: u16 = 1 << 1;

        for &byte in bytes {
            // SAFETY: this bring-up diagnostic runs only on WS63 after flashboot
            // configured UART0 at 115200 baud.
            unsafe {
                while core::ptr::read_volatile(ST) & TX_FULL != 0 {
                    core::hint::spin_loop();
                }
                core::ptr::write_volatile(DATA, byte as u16);
                while core::ptr::read_volatile(ST) & TX_EMPTY == 0 {
                    core::hint::spin_loop();
                }
            }
        }
    }
}

pub(crate) fn trace_nv(key: u16, max_len: u16, actual_len: u16, result: u32) {
    diag_emit(b"RFDBG_NV key=0x");
    diag_emit(&hex8(key as u32));
    diag_emit(b" max=0x");
    diag_emit(&hex8(max_len as u32));
    diag_emit(b" len=0x");
    diag_emit(&hex8(actual_len as u32));
    diag_emit(b" ret=0x");
    diag_emit(&hex8(result));
    diag_emit(b"\r\n");
}

pub(crate) fn trace_bad_free(ptr: u32, total: u32, magic: u32, caller: u32) {
    diag_emit(b"RFDBG_BAD_FREE ptr=0x");
    diag_emit(&hex8(ptr));
    diag_emit(b" total=0x");
    diag_emit(&hex8(total));
    diag_emit(b" magic=0x");
    diag_emit(&hex8(magic));
    diag_emit(b" ra=0x");
    diag_emit(&hex8(caller));
    diag_emit(b"\r\n");
}

pub(crate) fn trace_task(event: &[u8], slot: usize, entry: usize, arg: usize, stack_size: usize) {
    diag_emit(b"RFDBG_TASK ");
    diag_emit(event);
    diag_emit(b" slot=0x");
    diag_emit(&hex8(slot as u32));
    diag_emit(b" entry=0x");
    diag_emit(&hex8(entry as u32));
    diag_emit(b" arg=0x");
    diag_emit(&hex8(arg as u32));
    diag_emit(b" stack=0x");
    diag_emit(&hex8(stack_size as u32));
    diag_emit(b"\r\n");
}

pub(crate) fn trace_wait(
    event: &[u8],
    task: usize,
    wait: usize,
    timeout_ms: u32,
    result: i32,
    caller: usize,
) {
    diag_emit(b"RFDBG_WAIT ");
    diag_emit(event);
    diag_emit(b" task=0x");
    diag_emit(&hex8(task as u32));
    diag_emit(b" wait=0x");
    diag_emit(&hex8(wait as u32));
    diag_emit(b" timeout=0x");
    diag_emit(&hex8(timeout_ms));
    diag_emit(b" ret=0x");
    diag_emit(&hex8(result as u32));
    diag_emit(b" ra=0x");
    diag_emit(&hex8(caller as u32));
    diag_emit(b"\r\n");
}

pub(crate) fn trace_timer(
    event: &[u8],
    timer: usize,
    handler: usize,
    data: usize,
    interval_ms: u32,
) {
    diag_emit(b"RFDBG_TIMER ");
    diag_emit(event);
    diag_emit(b" timer=0x");
    diag_emit(&hex8(timer as u32));
    diag_emit(b" handler=0x");
    diag_emit(&hex8(handler as u32));
    diag_emit(b" data=0x");
    diag_emit(&hex8(data as u32));
    diag_emit(b" interval=0x");
    diag_emit(&hex8(interval_ms));
    diag_emit(b"\r\n");
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wrap_hmac_main_init_etc() -> i32 {
    let result = unsafe { __real_hmac_main_init_etc() };
    trace_stage(b"hmac_main", result as u32);
    result
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wrap_wal_main_init() -> i32 {
    let result = unsafe { __real_wal_main_init() };
    trace_stage(b"wal_main", result as u32);
    result
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hmac_main_init_later() -> i32 {
    let result = unsafe { __ws63_vendor_hmac_main_init_later() };
    trace_stage(b"hmac_later", result as u32);
    result
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wrap_wal_customize_set_config() -> u32 {
    let result = unsafe { __real_wal_customize_set_config() };
    trace_stage(b"wal_customize", result);
    result
}

fn trace_message(kind: &[u8], msg_id: u16, len: u16, result: u32) {
    diag_emit(b"RFDBG_MSG ");
    diag_emit(kind);
    diag_emit(b" id=0x");
    diag_emit(&hex8(msg_id as u32));
    diag_emit(b" len=0x");
    diag_emit(&hex8(len as u32));
    diag_emit(b" ret=0x");
    diag_emit(&hex8(result));
    diag_emit(b"\r\n");
}

#[cfg(target_arch = "riscv32")]
unsafe fn trace_c_string(value: *const u8) {
    if value.is_null() {
        diag_emit(b"<null>");
        return;
    }
    for index in 0..32 {
        // SAFETY: the vendor ABI supplies a NUL-terminated interface name;
        // limit the diagnostic read to its 32-byte interface-name bound.
        let byte = unsafe { value.add(index).read() };
        if byte == 0 {
            return;
        }
        diag_emit(core::slice::from_ref(&byte));
    }
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oal_get_netdev_by_name(name: *const u8) -> *mut core::ffi::c_void {
    let result = unsafe { __ws63_vendor_oal_get_netdev_by_name(name) };
    diag_emit(b"RFDBG_NET lookup=");
    unsafe { trace_c_string(name) };
    diag_emit(b" result=0x");
    diag_emit(&hex8(result as usize as u32));
    diag_emit(b"\r\n");
    result
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oal_net_register_netdev(netdev: *mut core::ffi::c_void) -> u32 {
    let result = unsafe { __ws63_vendor_oal_net_register_netdev(netdev) };
    diag_emit(b"RFDBG_NET register=0x");
    diag_emit(&hex8(netdev as usize as u32));
    diag_emit(b" ret=0x");
    diag_emit(&hex8(result));
    diag_emit(b"\r\n");
    result
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wal_sync_post2hmac_no_rsp(
    vap_id: u8,
    msg_id: u16,
    data: *const u8,
    len: u16,
) -> u32 {
    let result = unsafe { __ws63_vendor_wal_sync_post2hmac_no_rsp(vap_id, msg_id, data, len) };
    trace_message(b"hmac", msg_id, len, result);
    result
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wal_sync_send2device_no_rsp(
    vap_id: u8,
    msg_id: u16,
    data: *const u8,
    len: u16,
) -> u32 {
    let result = unsafe { __ws63_vendor_wal_sync_send2device_no_rsp(vap_id, msg_id, data, len) };
    trace_message(b"device", msg_id, len, result);
    result
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn frw_sync_host_post_msg(
    msg_id: u16,
    vap_id: u8,
    timeout_ms: u16,
    msg: *mut core::ffi::c_void,
) -> i32 {
    diag_emit(b"RFDBG_FRW begin id=0x");
    diag_emit(&hex8(msg_id as u32));
    diag_emit(b" vap=0x");
    diag_emit(&hex8(vap_id as u32));
    diag_emit(b" timeout=0x");
    diag_emit(&hex8(timeout_ms as u32));
    diag_emit(b"\r\n");
    let result = unsafe { __ws63_vendor_frw_sync_host_post_msg(msg_id, vap_id, timeout_ms, msg) };
    diag_emit(b"RFDBG_FRW end id=0x");
    diag_emit(&hex8(msg_id as u32));
    diag_emit(b" ret=0x");
    diag_emit(&hex8(result as u32));
    diag_emit(b"\r\n");
    result
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn frw_send_cfg_to_device_sync(
    msg_id: u16,
    vap_id: u8,
    timeout_ms: u16,
    msg: *mut core::ffi::c_void,
) -> i32 {
    diag_emit(b"RFDBG_DMAC begin id=0x");
    diag_emit(&hex8(msg_id as u32));
    diag_emit(b" vap=0x");
    diag_emit(&hex8(vap_id as u32));
    diag_emit(b" timeout=0x");
    diag_emit(&hex8(timeout_ms as u32));
    diag_emit(b"\r\n");
    let result =
        unsafe { __ws63_vendor_frw_send_cfg_to_device_sync(msg_id, vap_id, timeout_ms, msg) };
    diag_emit(b"RFDBG_DMAC end id=0x");
    diag_emit(&hex8(msg_id as u32));
    diag_emit(b" ret=0x");
    diag_emit(&hex8(result as u32));
    diag_emit(b"\r\n");
    result
}
