//! Low-disturbance authentication and WPA event-loop counters for on-silicon diagnosis.

use core::cell::RefCell;
#[cfg(target_arch = "riscv32")]
use core::ffi::c_void;
use critical_section::Mutex;

#[cfg(all(target_arch = "riscv32", feature = "wifi-personal"))]
unsafe extern "C" {
    fn __ws63_vendor_eloop_post_event(
        event: *mut c_void,
        buffer: *mut c_void,
        set_event: i32,
    ) -> i32;
    fn __ws63_vendor_eloop_read_event(event: *mut c_void, timeout: i32) -> *mut c_void;
    fn drv_soc_driver_event_process();
    fn drv_soc_driver_ap_event_process();
    fn __ws63_vendor_wpa_supplicant_event(ctx: *mut c_void, event: i32, data: *mut c_void);
    fn los_get_wifi_dev_by_priv(ctx: *const c_void) -> *mut c_void;
    fn eloop_is_running(task: i32) -> i32;
}

#[cfg(target_arch = "riscv32")]
unsafe extern "C" {
    fn __ws63_vendor_hmac_sta_wait_auth_seq2_rx_etc(vap: *mut c_void, message: *mut c_void) -> u32;
    fn __ws63_vendor_hmac_sta_auth_timeout_etc(vap: *mut c_void, parameter: *mut c_void) -> u32;
    fn __ws63_vendor_hmac_rx_mgmt_event_adapt(vap: *mut c_void, message: *mut c_void) -> i32;
    fn __ws63_vendor_hmac_tx_mgmt_send_event_etc(
        vap: *mut c_void,
        netbuf: *mut c_void,
        frame_len: u16,
    ) -> u32;
    fn oal_netbuf_mac_header(netbuf: *const c_void) -> *const u8;
    fn hmac_bridge_vap_xmit_etc(vap: *mut c_void, message: *mut c_void) -> i32;
    fn hwal_netif_rx(netdev: *mut c_void, netbuf: *mut c_void) -> u32;
    fn __ws63_vendor_dmac_rx_prepare_data_patch(
        netbuf: *mut c_void,
        rx_ctl: *mut c_void,
        vap_id: u32,
        rx_status: *mut c_void,
        process_flag: *mut c_void,
    ) -> u32;
    fn hh503_get_mac_rx_statistics_data(statistics: *mut MacRxStatistics);
    #[cfg(feature = "rf-auth-scan-filter")]
    fn hal_set_rx_filter_reg(command: u32);
}

const DIAGNOSTIC_SLOTS: usize = 16;
const DRIVER_EVENT_SLOTS: usize = 32;
const AUTH_TIMEOUT_SLOTS: usize = 8;
const TX_COMPLETE_WORDS: usize = 12;
const AUTH_FRAME_BYTES: usize = 32;
const TX_FRAME_PREFIX_BYTES: usize = 32;

/// Read-only per-queue counters for the guarded WPA event-loop build.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EloopDiagnostic {
    pub event: usize,
    pub posts: u32,
    pub post_failures: u32,
    pub reads: u32,
    pub nonempty_reads: u32,
    pub last_post_caller: usize,
    pub last_read_caller: usize,
    pub last_buffer: usize,
}

const EMPTY: EloopDiagnostic = EloopDiagnostic {
    event: 0,
    posts: 0,
    post_failures: 0,
    reads: 0,
    nonempty_reads: 0,
    last_post_caller: 0,
    last_read_caller: 0,
    last_buffer: 0,
};

static DIAGNOSTICS: Mutex<RefCell<[EloopDiagnostic; DIAGNOSTIC_SLOTS]>> =
    Mutex::new(RefCell::new([EMPTY; DIAGNOSTIC_SLOTS]));

/// One packet consumed by the WPA driver-event dispatcher.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DriverEventDiagnostic {
    pub sequence: u32,
    pub command: u32,
    pub length: u32,
    pub payload0: u32,
}

struct DriverEventHistory {
    entries: [DriverEventDiagnostic; DRIVER_EVENT_SLOTS],
    next: usize,
    total: u32,
}

static DRIVER_EVENTS: Mutex<RefCell<DriverEventHistory>> =
    Mutex::new(RefCell::new(DriverEventHistory {
        entries: [DriverEventDiagnostic {
            sequence: 0,
            command: 0,
            length: 0,
            payload0: 0,
        }; DRIVER_EVENT_SLOTS],
        next: 0,
        total: 0,
    }));

/// Last entry into the WPA supplicant event dispatcher.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SupplicantEventDiagnostic {
    pub calls: u32,
    pub event: i32,
    pub context: usize,
    pub wifi_device: usize,
    pub eloop_status: i32,
}

/// Register values observed at the dispatcher's secure-C clear call.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DriverDispatchDiagnostic {
    pub samples: u32,
    pub caller: usize,
    pub command_register: usize,
    pub length_register: usize,
}

/// Raw MAC/BSSID receive-filter registers for one hardware VAP.
///
/// The vendor ROM stores the first two station-address bytes in the upper half
/// of `combined` and the first two BSSID bytes in its lower half. The remaining
/// four bytes live in `station_tail` and `bssid_tail`, respectively.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MacFilterDiagnostic {
    pub combined: u32,
    pub station_tail: u32,
    pub bssid_tail: u32,
}

/// Hardware MAC receive counters read by the vendor ROM helper.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MacRxStatistics {
    pub ampdu: u32,
    pub successful_mpdu_in_ampdu: u32,
    pub failed_mpdu_in_ampdu: u32,
    pub successful_mpdu: u32,
    pub failed_mpdu: u32,
    pub filtered_mpdu: u32,
}

static DRIVER_DISPATCH: Mutex<RefCell<DriverDispatchDiagnostic>> =
    Mutex::new(RefCell::new(DriverDispatchDiagnostic {
        samples: 0,
        caller: 0,
        command_register: 0,
        length_register: 0,
    }));

static SUPPLICANT_EVENT: Mutex<RefCell<SupplicantEventDiagnostic>> =
    Mutex::new(RefCell::new(SupplicantEventDiagnostic {
        calls: 0,
        event: 0,
        context: 0,
        wifi_device: 0,
        eloop_status: 0,
    }));

/// Authentication state-machine entries observed by the diagnostic build.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AuthDiagnostic {
    pub dmac_rx_calls: u32,
    pub dmac_rx_auth_frames: u32,
    pub dmac_rx_auth_seq2_frames: u32,
    pub hmac_ingress_calls: u32,
    pub hmac_ingress_auth_frames: u32,
    pub hmac_ingress_auth_seq2_frames: u32,
    pub tx_auth_frames: u32,
    pub tx_algorithm: u16,
    pub tx_sequence: u16,
    pub tx_netbuf: usize,
    pub tx_destination: [u8; 6],
    pub tx_source: [u8; 6],
    pub tx_bssid: [u8; 6],
    pub tx_frame_len: u16,
    pub tx_frame: [u8; AUTH_FRAME_BYTES],
    pub tx_vap_id: u8,
    pub tx_mac_filter: MacFilterDiagnostic,
    pub rx_statistics_at_tx: MacRxStatistics,
    pub rx_statistics_at_timeout: MacRxStatistics,
    pub rx_filter_before_tx: u32,
    pub rx_filter_after_override: u32,
    pub tx_complete_calls: u32,
    pub tx_complete_after_auth: u32,
    pub tx_complete_skb: usize,
    pub tx_complete_frame: usize,
    pub tx_complete_status: u8,
    pub tx_complete_data_counts: u16,
    pub tx_complete_frame_prefix: [u8; TX_FRAME_PREFIX_BYTES],
    pub bridge_xmit_calls: u32,
    pub bridge_xmit_result: i32,
    pub bridge_xmit_skb: usize,
    pub netif_rx_calls: u32,
    pub netif_rx_eapol_frames: u32,
    pub netif_rx_length: u32,
    pub netif_rx_prefix: [u8; TX_FRAME_PREFIX_BYTES],
    pub netif_rx_eapol_length: u32,
    pub netif_rx_eapol_prefix: [u8; TX_FRAME_PREFIX_BYTES],
    pub auth_tx_complete_calls: u32,
    pub auth_tx_status: u8,
    pub auth_tx_data_counts: u16,
    pub last_tx_complete_words: [u32; TX_COMPLETE_WORDS],
    pub wait_state_calls: u32,
    pub auth_frames: u32,
    pub auth_seq2_frames: u32,
    pub last_algorithm: u16,
    pub last_sequence: u16,
    pub last_status: u16,
    pub last_handler_result: u32,
    pub timeout_calls: u32,
    pub first_auth_seq2_systick_ms: u64,
    pub last_auth_seq2_systick_ms: u64,
    pub first_auth_seq2_tcxo_ms: u64,
    pub last_auth_seq2_tcxo_ms: u64,
    pub timeout_systick_ms: [u64; AUTH_TIMEOUT_SLOTS],
    pub timeout_tcxo_ms: [u64; AUTH_TIMEOUT_SLOTS],
}

static AUTH: Mutex<RefCell<AuthDiagnostic>> = Mutex::new(RefCell::new(AuthDiagnostic {
    dmac_rx_calls: 0,
    dmac_rx_auth_frames: 0,
    dmac_rx_auth_seq2_frames: 0,
    hmac_ingress_calls: 0,
    hmac_ingress_auth_frames: 0,
    hmac_ingress_auth_seq2_frames: 0,
    tx_auth_frames: 0,
    tx_algorithm: 0,
    tx_sequence: 0,
    tx_netbuf: 0,
    tx_destination: [0; 6],
    tx_source: [0; 6],
    tx_bssid: [0; 6],
    tx_frame_len: 0,
    tx_frame: [0; AUTH_FRAME_BYTES],
    tx_vap_id: 0,
    tx_mac_filter: MacFilterDiagnostic {
        combined: 0,
        station_tail: 0,
        bssid_tail: 0,
    },
    rx_statistics_at_tx: MacRxStatistics {
        ampdu: 0,
        successful_mpdu_in_ampdu: 0,
        failed_mpdu_in_ampdu: 0,
        successful_mpdu: 0,
        failed_mpdu: 0,
        filtered_mpdu: 0,
    },
    rx_statistics_at_timeout: MacRxStatistics {
        ampdu: 0,
        successful_mpdu_in_ampdu: 0,
        failed_mpdu_in_ampdu: 0,
        successful_mpdu: 0,
        failed_mpdu: 0,
        filtered_mpdu: 0,
    },
    rx_filter_before_tx: 0,
    rx_filter_after_override: 0,
    tx_complete_calls: 0,
    tx_complete_after_auth: 0,
    tx_complete_skb: 0,
    tx_complete_frame: 0,
    tx_complete_status: 0,
    tx_complete_data_counts: 0,
    tx_complete_frame_prefix: [0; TX_FRAME_PREFIX_BYTES],
    bridge_xmit_calls: 0,
    bridge_xmit_result: 0,
    bridge_xmit_skb: 0,
    netif_rx_calls: 0,
    netif_rx_eapol_frames: 0,
    netif_rx_length: 0,
    netif_rx_prefix: [0; TX_FRAME_PREFIX_BYTES],
    netif_rx_eapol_length: 0,
    netif_rx_eapol_prefix: [0; TX_FRAME_PREFIX_BYTES],
    auth_tx_complete_calls: 0,
    auth_tx_status: 0,
    auth_tx_data_counts: 0,
    last_tx_complete_words: [0; TX_COMPLETE_WORDS],
    wait_state_calls: 0,
    auth_frames: 0,
    auth_seq2_frames: 0,
    last_algorithm: 0,
    last_sequence: 0,
    last_status: 0,
    last_handler_result: 0,
    timeout_calls: 0,
    first_auth_seq2_systick_ms: 0,
    last_auth_seq2_systick_ms: 0,
    first_auth_seq2_tcxo_ms: 0,
    last_auth_seq2_tcxo_ms: 0,
    timeout_systick_ms: [0; AUTH_TIMEOUT_SLOTS],
    timeout_tcxo_ms: [0; AUTH_TIMEOUT_SLOTS],
}));

/// Observe the first call after `uapi_lwip_send` converts a host pbuf to a
/// vendor skb. Absence of this call means the allocation/conversion path failed.
#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn __ws63_diag_hmac_bridge_vap_xmit_etc(
    vap: *mut c_void,
    message: *mut c_void,
) -> i32 {
    let skb = if message.is_null() {
        0
    } else {
        let data = unsafe { message.cast::<*const usize>().read_unaligned() };
        if data.is_null() {
            0
        } else {
            unsafe { data.read_unaligned() }
        }
    };
    let result = unsafe { hmac_bridge_vap_xmit_etc(vap, message) };
    critical_section::with(|cs| {
        let mut diagnostic = AUTH.borrow_ref_mut(cs);
        diagnostic.bridge_xmit_calls = diagnostic.bridge_xmit_calls.saturating_add(1);
        diagnostic.bridge_xmit_result = result;
        diagnostic.bridge_xmit_skb = skb;
    });
    result
}

/// Observe frames at the final vendor host-RX boundary before skb-to-pbuf
/// conversion. The wrapper forwards the exact `hwal_netif_rx` ABI unchanged.
#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn __ws63_diag_hwal_netif_rx(
    netdev: *mut c_void,
    netbuf: *mut c_void,
) -> u32 {
    let mut length = 0;
    let mut prefix = [0; TX_FRAME_PREFIX_BYTES];
    if !netbuf.is_null() {
        // WS63 LiteOS `struct sk_buff`: len is at +12 and data at +84. These
        // offsets are shared with `netbuf_frame` and checked against the SDK
        // header used to build the vendor archive.
        length = unsafe { netbuf.cast::<u8>().add(12).cast::<u32>().read_unaligned() };
        let data = unsafe {
            netbuf
                .cast::<u8>()
                .add(84)
                .cast::<*const u8>()
                .read_unaligned()
        };
        if !data.is_null() {
            let copied = usize::min(length as usize, prefix.len());
            unsafe { core::ptr::copy_nonoverlapping(data, prefix.as_mut_ptr(), copied) };
        }
    }
    critical_section::with(|cs| {
        let mut diagnostic = AUTH.borrow_ref_mut(cs);
        diagnostic.netif_rx_calls = diagnostic.netif_rx_calls.saturating_add(1);
        diagnostic.netif_rx_length = length;
        diagnostic.netif_rx_prefix = prefix;
        if length >= 14 && prefix[12..14] == [0x88, 0x8e] {
            diagnostic.netif_rx_eapol_frames = diagnostic.netif_rx_eapol_frames.saturating_add(1);
            diagnostic.netif_rx_eapol_length = length;
            diagnostic.netif_rx_eapol_prefix = prefix;
        }
    });
    unsafe { hwal_netif_rx(netdev, netbuf) }
}

#[cfg(all(target_arch = "riscv32", feature = "wifi-personal"))]
fn update(event: usize, apply: impl FnOnce(&mut EloopDiagnostic)) {
    critical_section::with(|cs| {
        let mut diagnostics = DIAGNOSTICS.borrow_ref_mut(cs);
        let slot = diagnostics
            .iter()
            .position(|entry| entry.event == event)
            .or_else(|| diagnostics.iter().position(|entry| entry.event == 0));
        if let Some(slot) = slot.map(|index| &mut diagnostics[index]) {
            if slot.event == 0 {
                slot.event = event;
            }
            apply(slot);
        }
    });
}

#[cfg(all(target_arch = "riscv32", feature = "wifi-personal"))]
fn record_driver_event(command: u32, length: u32, payload0: u32) {
    critical_section::with(|cs| {
        let mut history = DRIVER_EVENTS.borrow_ref_mut(cs);
        history.total = history.total.saturating_add(1);
        let index = history.next;
        history.entries[index] = DriverEventDiagnostic {
            sequence: history.total,
            command,
            length,
            payload0,
        };
        history.next = (index + 1) % DRIVER_EVENT_SLOTS;
    });
}

/// Copies WPA event-loop counters without changing queue state.
pub fn diagnostics(output: &mut [EloopDiagnostic]) -> usize {
    critical_section::with(|cs| {
        let diagnostics = DIAGNOSTICS.borrow_ref(cs);
        let mut count = 0;
        for diagnostic in diagnostics.iter().copied().filter(|entry| entry.event != 0) {
            if count == output.len() {
                break;
            }
            output[count] = diagnostic;
            count += 1;
        }
        count
    })
}

/// Copies the retained WPA driver-event packets in chronological order.
pub fn driver_events(output: &mut [DriverEventDiagnostic]) -> usize {
    critical_section::with(|cs| {
        let history = DRIVER_EVENTS.borrow_ref(cs);
        let retained = usize::min(history.total as usize, DRIVER_EVENT_SLOTS);
        let count = usize::min(retained, output.len());
        let oldest = if history.total as usize > DRIVER_EVENT_SLOTS {
            history.next
        } else {
            0
        };
        for (destination, offset) in output.iter_mut().zip(0..count) {
            *destination = history.entries[(oldest + offset) % DRIVER_EVENT_SLOTS];
        }
        count
    })
}

/// Returns the latest WPA supplicant dispatcher entry.
pub fn supplicant_event() -> SupplicantEventDiagnostic {
    critical_section::with(|cs| *SUPPLICANT_EVENT.borrow_ref(cs))
}

/// Returns the latest register sample from the driver-event dispatcher.
pub fn driver_dispatch() -> DriverDispatchDiagnostic {
    critical_section::with(|cs| *DRIVER_DISPATCH.borrow_ref(cs))
}

/// Returns authentication RX/timeout state-machine counters.
pub fn auth() -> AuthDiagnostic {
    critical_section::with(|cs| *AUTH.borrow_ref(cs))
}

/// Snapshots the MAC/BSSID receive-filter registers for `vap_id`.
///
/// These addresses and their three-VAP table are taken from the WS63 mask-ROM
/// implementations of `hh503_vap_set_macaddr` and `hh503_set_sta_bssid`.
#[cfg(target_arch = "riscv32")]
pub fn mac_filter(vap_id: usize) -> Option<MacFilterDiagnostic> {
    const COMBINED: [usize; 3] = [0x4421_0404, 0x4421_042c, 0x4421_0524];
    const STATION_TAIL: [usize; 3] = [0x4421_0400, 0x4421_0428, 0x4421_0520];
    const BSSID_TAIL: [usize; 3] = [0x4421_0408, 0x4421_0430, 0x4421_0528];

    let combined = *COMBINED.get(vap_id)? as *const u32;
    let station_tail = STATION_TAIL[vap_id] as *const u32;
    let bssid_tail = BSSID_TAIL[vap_id] as *const u32;
    // SAFETY: all three addresses are readable MAC-control registers. The ROM
    // setters themselves read `combined` before updating one of its halfwords.
    Some(unsafe {
        MacFilterDiagnostic {
            combined: combined.read_volatile(),
            station_tail: station_tail.read_volatile(),
            bssid_tail: bssid_tail.read_volatile(),
        }
    })
}

#[cfg(target_arch = "riscv32")]
fn mac_rx_statistics() -> MacRxStatistics {
    let mut statistics = MacRxStatistics::default();
    // SAFETY: the ROM helper writes exactly the six u32 fields declared by
    // `hal_mac_rx_mpdu_statis_info_stru` and only reads MAC counter registers.
    unsafe { hh503_get_mac_rx_statistics_data(&mut statistics) };
    statistics
}

#[cfg(target_arch = "riscv32")]
unsafe fn netbuf_frame(netbuf: *const u8) -> Option<*const u8> {
    if netbuf.is_null() {
        return None;
    }
    let frame = unsafe { netbuf.add(84).cast::<*const u8>().read_unaligned() };
    (!frame.is_null()).then_some(frame)
}

#[cfg(target_arch = "riscv32")]
unsafe fn classify_auth_frame(netbuf: *const u8) -> Option<(u16, u16, u16)> {
    let frame = unsafe { netbuf_frame(netbuf) }?;
    if unsafe { frame.read() } & 0xfc != 0xb0 {
        return None;
    }
    Some((
        unsafe { frame.add(24).cast::<u16>().read_unaligned() },
        unsafe { frame.add(26).cast::<u16>().read_unaligned() },
        unsafe { frame.add(28).cast::<u16>().read_unaligned() },
    ))
}

#[cfg(target_arch = "riscv32")]
unsafe fn message_netbuf(message: *mut c_void) -> *const u8 {
    if message.is_null() {
        core::ptr::null()
    } else {
        // The management-frame `frw_msg.data` points to one word containing
        // the host netbuf pointer. This matches both vendor consumers:
        // `hmac_rx_mgmt_event_adapt` and `hmac_sta_wait_auth_seq2_rx_etc` first
        // load `msg->data`, then load the netbuf from that storage.
        let data = unsafe { message.cast::<*const *const u8>().read_unaligned() };
        if data.is_null() {
            core::ptr::null()
        } else {
            unsafe { data.cast::<*const u8>().read_unaligned() }
        }
    }
}

#[cfg(all(target_arch = "riscv32", feature = "wifi-personal"))]
#[inline(always)]
fn return_address() -> usize {
    let caller: usize;
    // SAFETY: reads the return-address register without touching memory.
    unsafe {
        core::arch::asm!("mv {caller}, ra", caller = out(reg) caller, options(nomem, nostack));
    }
    caller
}

#[cfg(all(target_arch = "riscv32", feature = "wifi-personal"))]
fn is_driver_event_consumer(caller: usize) -> bool {
    let start = drv_soc_driver_event_process as *const () as usize;
    let end = drv_soc_driver_ap_event_process as *const () as usize;
    start < end && (start..end).contains(&caller)
}

/// Records the live dispatcher registers without changing vendor control flow.
#[cfg(all(target_arch = "riscv32", feature = "wifi-personal"))]
pub(crate) fn record_dispatch_registers(caller: usize, command: usize, length: usize) {
    if !is_driver_event_consumer(caller) {
        return;
    }
    critical_section::with(|cs| {
        let mut diagnostic = DRIVER_DISPATCH.borrow_ref_mut(cs);
        diagnostic.samples = diagnostic.samples.saturating_add(1);
        diagnostic.caller = caller;
        diagnostic.command_register = command;
        diagnostic.length_register = length;
    });
}

#[cfg(all(target_arch = "riscv32", not(feature = "wifi-personal")))]
pub(crate) fn record_dispatch_registers(_: usize, _: usize, _: usize) {}

/// Preserve vendor queueing while counting event-loop producers.
#[cfg(all(target_arch = "riscv32", feature = "wifi-personal"))]
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn eloop_post_event(
    event: *mut c_void,
    buffer: *mut c_void,
    set_event: i32,
) -> i32 {
    let caller = return_address();
    // SAFETY: the diagnostic build renames the exact vendor function and this
    // wrapper forwards its ABI and arguments unchanged.
    let result = unsafe { __ws63_vendor_eloop_post_event(event, buffer, set_event) };
    update(event as usize, |entry| {
        entry.posts = entry.posts.saturating_add(1);
        entry.post_failures = entry.post_failures.saturating_add((result != 0) as u32);
        entry.last_post_caller = caller;
        entry.last_buffer = buffer as usize;
    });
    result
}

/// Preserve vendor dequeueing while counting event-loop consumers.
#[cfg(all(target_arch = "riscv32", feature = "wifi-personal"))]
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn eloop_read_event(event: *mut c_void, timeout: i32) -> *mut c_void {
    let caller = return_address();
    // SAFETY: the diagnostic build renames the exact vendor function and this
    // wrapper forwards its ABI and arguments unchanged.
    let result = unsafe { __ws63_vendor_eloop_read_event(event, timeout) };
    if !result.is_null() && is_driver_event_consumer(caller) {
        let words = result.cast::<u32>();
        // SAFETY: the vendor driver's queue packet begins with two u32 header
        // fields and `length` payload bytes. Classification by the exported
        // dispatcher boundaries prevents reading unrelated eloop payloads.
        let command = unsafe { words.read_unaligned() };
        let length = unsafe { words.add(1).read_unaligned() };
        let payload0 = if length >= 4 {
            unsafe { words.add(2).read_unaligned() }
        } else {
            0
        };
        record_driver_event(command, length, payload0);
    }
    update(event as usize, |entry| {
        entry.reads = entry.reads.saturating_add(1);
        entry.nonempty_reads = entry
            .nonempty_reads
            .saturating_add((!result.is_null()) as u32);
        entry.last_read_caller = caller;
        if !result.is_null() {
            entry.last_buffer = result as usize;
        }
    });
    result
}

/// Preserve the vendor dispatcher while recording its two early-return gates.
#[cfg(all(target_arch = "riscv32", feature = "wifi-personal"))]
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn wpa_supplicant_event(ctx: *mut c_void, event: i32, data: *mut c_void) {
    // SAFETY: both functions are vendor C APIs accepting this opaque context.
    let wifi_device = unsafe { los_get_wifi_dev_by_priv(ctx) };
    let eloop_status = unsafe { eloop_is_running(0) };
    critical_section::with(|cs| {
        let mut diagnostic = SUPPLICANT_EVENT.borrow_ref_mut(cs);
        diagnostic.calls = diagnostic.calls.saturating_add(1);
        diagnostic.event = event;
        diagnostic.context = ctx as usize;
        diagnostic.wifi_device = wifi_device as usize;
        diagnostic.eloop_status = eloop_status;
    });
    // SAFETY: the build rewrites the matching vendor definition and this
    // wrapper forwards all arguments with the original ABI.
    unsafe { __ws63_vendor_wpa_supplicant_event(ctx, event, data) };
}

/// Preserve the vendor wait-auth handler while classifying management frames.
#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn hmac_sta_wait_auth_seq2_rx_etc(
    vap: *mut c_void,
    message: *mut c_void,
) -> u32 {
    let systick_ms = crate::uapi::monotonic_ms();
    let tcxo_ms = crate::uapi::monotonic_us() / 1_000;
    let auth = unsafe { classify_auth_frame(message_netbuf(message)) };
    critical_section::with(|cs| {
        let mut diagnostic = AUTH.borrow_ref_mut(cs);
        diagnostic.wait_state_calls = diagnostic.wait_state_calls.saturating_add(1);
        if let Some((algorithm, sequence, status)) = auth {
            diagnostic.auth_frames = diagnostic.auth_frames.saturating_add(1);
            diagnostic.last_algorithm = algorithm;
            diagnostic.last_sequence = sequence;
            diagnostic.last_status = status;
            if sequence == 2 {
                diagnostic.auth_seq2_frames = diagnostic.auth_seq2_frames.saturating_add(1);
                if diagnostic.first_auth_seq2_systick_ms == 0 {
                    diagnostic.first_auth_seq2_systick_ms = systick_ms;
                    diagnostic.first_auth_seq2_tcxo_ms = tcxo_ms;
                }
                diagnostic.last_auth_seq2_systick_ms = systick_ms;
                diagnostic.last_auth_seq2_tcxo_ms = tcxo_ms;
            }
        }
    });
    // SAFETY: the build renames the exact vendor symbol and this forwards its ABI.
    let result = unsafe { __ws63_vendor_hmac_sta_wait_auth_seq2_rx_etc(vap, message) };
    critical_section::with(|cs| AUTH.borrow_ref_mut(cs).last_handler_result = result);
    result
}

/// Observe management frames at the HMAC ingress before asynchronous FSM
/// dispatch, while preserving the vendor adapter unchanged.
#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn hmac_rx_mgmt_event_adapt(vap: *mut c_void, message: *mut c_void) -> i32 {
    let auth = unsafe { classify_auth_frame(message_netbuf(message)) };
    critical_section::with(|cs| {
        let mut diagnostic = AUTH.borrow_ref_mut(cs);
        diagnostic.hmac_ingress_calls = diagnostic.hmac_ingress_calls.saturating_add(1);
        if let Some((_, sequence, _)) = auth {
            diagnostic.hmac_ingress_auth_frames =
                diagnostic.hmac_ingress_auth_frames.saturating_add(1);
            diagnostic.hmac_ingress_auth_seq2_frames = diagnostic
                .hmac_ingress_auth_seq2_frames
                .saturating_add((sequence == 2) as u32);
        }
    });
    // SAFETY: the build renames the exact vendor symbol and this forwards its ABI.
    unsafe { __ws63_vendor_hmac_rx_mgmt_event_adapt(vap, message) }
}

/// Observe every device-side RX frame before DMAC filtering and upload.
///
/// This wraps the vendor replacement already selected by the standard WS63
/// mask-ROM patch list, so enabling diagnostics does not consume another
/// hardware patch comparator.
#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn dmac_rx_prepare_data_patch(
    netbuf: *mut c_void,
    rx_ctl: *mut c_void,
    vap_id: u32,
    rx_status: *mut c_void,
    process_flag: *mut c_void,
) -> u32 {
    let frame = if netbuf.is_null() {
        core::ptr::null()
    } else {
        unsafe { oal_netbuf_mac_header(netbuf) }
    };
    let auth_sequence = if !frame.is_null() && unsafe { frame.read() } & 0xfc == 0xb0 {
        Some(unsafe { frame.add(26).cast::<u16>().read_unaligned() })
    } else {
        None
    };
    critical_section::with(|cs| {
        let mut diagnostic = AUTH.borrow_ref_mut(cs);
        diagnostic.dmac_rx_calls = diagnostic.dmac_rx_calls.saturating_add(1);
        if let Some(sequence) = auth_sequence {
            diagnostic.dmac_rx_auth_frames = diagnostic.dmac_rx_auth_frames.saturating_add(1);
            diagnostic.dmac_rx_auth_seq2_frames = diagnostic
                .dmac_rx_auth_seq2_frames
                .saturating_add((sequence == 2) as u32);
        }
    });
    // SAFETY: the diagnostic build renames the exact five-argument vendor
    // replacement and this wrapper forwards its ABI and arguments unchanged.
    unsafe {
        __ws63_vendor_dmac_rx_prepare_data_patch(netbuf, rx_ctl, vap_id, rx_status, process_flag)
    }
}

/// Record the exact authentication request passed to the vendor TX path.
#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn hmac_tx_mgmt_send_event_etc(
    vap: *mut c_void,
    netbuf: *mut c_void,
    frame_len: u16,
) -> u32 {
    let frame = unsafe { netbuf_frame(netbuf.cast()) };
    let auth = unsafe { classify_auth_frame(netbuf.cast()) };
    if let (Some(frame), Some((algorithm, sequence, _))) = (frame, auth) {
        let mut destination = [0; 6];
        let mut source = [0; 6];
        let mut bssid = [0; 6];
        let mut tx_frame = [0; AUTH_FRAME_BYTES];
        let capture_len = usize::min(frame_len as usize, tx_frame.len());
        unsafe {
            core::ptr::copy_nonoverlapping(frame.add(4), destination.as_mut_ptr(), 6);
            core::ptr::copy_nonoverlapping(frame.add(10), source.as_mut_ptr(), 6);
            core::ptr::copy_nonoverlapping(frame.add(16), bssid.as_mut_ptr(), 6);
            core::ptr::copy_nonoverlapping(frame, tx_frame.as_mut_ptr(), capture_len);
        }
        let vap_id = if vap.is_null() {
            0xff
        } else {
            (unsafe { vap.cast::<u32>().read_unaligned() }) as u8 & 0x0f
        };
        let mac_filter = mac_filter(vap_id as usize).unwrap_or_default();
        let rx_statistics = mac_rx_statistics();
        let rx_filter_before = unsafe { (0x4421_0048 as *const u32).read_volatile() };
        #[cfg(feature = "rf-auth-scan-filter")]
        unsafe {
            // Use the same command as `hal_device_state_scan_set_rx_filter_reg`.
            // This is a diagnostic differential, not a production workaround.
            hal_set_rx_filter_reg(0x27);
        }
        let rx_filter_after = unsafe { (0x4421_0048 as *const u32).read_volatile() };
        critical_section::with(|cs| {
            let mut diagnostic = AUTH.borrow_ref_mut(cs);
            diagnostic.tx_auth_frames = diagnostic.tx_auth_frames.saturating_add(1);
            diagnostic.tx_algorithm = algorithm;
            diagnostic.tx_sequence = sequence;
            diagnostic.tx_netbuf = netbuf as usize;
            diagnostic.tx_destination = destination;
            diagnostic.tx_source = source;
            diagnostic.tx_bssid = bssid;
            diagnostic.tx_frame_len = frame_len;
            diagnostic.tx_frame = tx_frame;
            diagnostic.tx_vap_id = vap_id;
            diagnostic.tx_mac_filter = mac_filter;
            diagnostic.rx_statistics_at_tx = rx_statistics;
            diagnostic.rx_filter_before_tx = rx_filter_before;
            diagnostic.rx_filter_after_override = rx_filter_after;
        });
    }
    // SAFETY: the build renames the exact vendor symbol and this forwards its ABI.
    unsafe { __ws63_vendor_hmac_tx_mgmt_send_event_etc(vap, netbuf, frame_len) }
}

/// Observe the DMAC TX-completion callback registered for message `0x34`.
///
/// The guarded build redirects only the callback pointer in
/// `dmac_forward_main.c.obj` to this symbol. The original implementation is a
/// mask-ROM function at `0x0012_435a`; its ABI is
/// `fn(dmac_vap_stru *, frw_msg *) -> osal_s32` in the vendor 5.10 sources.
#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn __ws63_diag_dmac_tx_complete_event_handler(
    vap: *mut c_void,
    message: *mut c_void,
) -> i32 {
    let mut words = [0; TX_COMPLETE_WORDS];
    let mut descriptor_skb = 0;
    let mut descriptor_frame = 0;
    let mut descriptor_is_auth = false;
    let mut descriptor_status = 0;
    let mut descriptor_data_counts = 0;
    let mut descriptor_frame_prefix = [0; TX_FRAME_PREFIX_BYTES];
    if !message.is_null() {
        // `frw_msg.data` is its first field. The vendor callback immediately
        // dereferences the same pointer, so it is live for this read-only
        // snapshot when the callback is entered.
        let data = unsafe { message.cast::<*const u32>().read_unaligned() };
        if !data.is_null() {
            for (index, word) in words.iter_mut().enumerate() {
                *word = unsafe { data.add(index).read_unaligned() };
            }
            let descriptor = words[0] as *const u32;
            if !descriptor.is_null() {
                // `hal_tx_dscr_stru` is 16 bytes before its hardware `data`:
                // list head (8), skb pointer (4), length/q/flags (4). Hardware
                // descriptor word0 then carries retry counts and status.
                descriptor_skb = unsafe { descriptor.add(2).read_unaligned() } as usize;
                let control = unsafe { descriptor.add(4).read_unaligned() };
                descriptor_status = ((control >> 28) & 0x0f) as u8;
                descriptor_data_counts = ((control >> 4) & 0x0fff) as u16;
                if descriptor_skb != 0 {
                    // The device-side skb is `oal_dmac_netbuf_stru`, not the
                    // LiteOS host `struct sk_buff`. Reuse the mask-ROM helper
                    // that applies `pkt_buf_offset` and packet-RAM layout.
                    let frame = unsafe { oal_netbuf_mac_header(descriptor_skb as *const c_void) };
                    descriptor_frame = frame as usize;
                    if !frame.is_null() {
                        descriptor_is_auth = unsafe { frame.read() } & 0xfc == 0xb0;
                        // The completion callback still owns the packet buffer.
                        // Packet-RAM allocations have at least one MAC header
                        // and payload slot; retain a bounded prefix for later
                        // UART diagnosis outside this hot path.
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                frame,
                                descriptor_frame_prefix.as_mut_ptr(),
                                descriptor_frame_prefix.len(),
                            )
                        };
                    }
                }
            }
        }
    }
    critical_section::with(|cs| {
        let mut diagnostic = AUTH.borrow_ref_mut(cs);
        diagnostic.tx_complete_calls = diagnostic.tx_complete_calls.saturating_add(1);
        if diagnostic.tx_auth_frames != 0 {
            diagnostic.tx_complete_after_auth = diagnostic.tx_complete_after_auth.saturating_add(1);
        }
        diagnostic.tx_complete_skb = descriptor_skb;
        diagnostic.tx_complete_frame = descriptor_frame;
        diagnostic.tx_complete_status = descriptor_status;
        diagnostic.tx_complete_data_counts = descriptor_data_counts;
        diagnostic.tx_complete_frame_prefix = descriptor_frame_prefix;
        if descriptor_is_auth {
            diagnostic.auth_tx_complete_calls = diagnostic.auth_tx_complete_calls.saturating_add(1);
            diagnostic.auth_tx_status = descriptor_status;
            diagnostic.auth_tx_data_counts = descriptor_data_counts;
        }
        diagnostic.last_tx_complete_words = words;
    });

    let vendor: unsafe extern "C" fn(*mut c_void, *mut c_void) -> i32 =
        unsafe { core::mem::transmute(0x0012_435a_usize) };
    unsafe { vendor(vap, message) }
}

/// Preserve the vendor auth timeout handler while recording expiry timing.
#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn hmac_sta_auth_timeout_etc(
    vap: *mut c_void,
    parameter: *mut c_void,
) -> u32 {
    let systick_ms = crate::uapi::monotonic_ms();
    let tcxo_ms = crate::uapi::monotonic_us() / 1_000;
    let rx_statistics = mac_rx_statistics();
    critical_section::with(|cs| {
        let mut diagnostic = AUTH.borrow_ref_mut(cs);
        let slot = diagnostic.timeout_calls as usize;
        diagnostic.timeout_calls = diagnostic.timeout_calls.saturating_add(1);
        diagnostic.rx_statistics_at_timeout = rx_statistics;
        if slot < AUTH_TIMEOUT_SLOTS {
            diagnostic.timeout_systick_ms[slot] = systick_ms;
            diagnostic.timeout_tcxo_ms[slot] = tcxo_ms;
        }
    });
    // SAFETY: the linker wraps the exact vendor symbol and this forwards its ABI.
    unsafe { __ws63_vendor_hmac_sta_auth_timeout_etc(vap, parameter) }
}
