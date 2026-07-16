//! Native runtime hooks for the pinned upstream hostap port.
//!
//! The C port owns no scheduler and does not emulate LiteOS. It delegates only
//! the capabilities needed by the single `RadioRunner` through
//! `hisi-rf-rtos-driver`, the RF heap, the WS63 monotonic clock and the explicit
//! WS63 entropy backend.

use core::cell::UnsafeCell;
use core::ffi::{c_int, c_void};
use core::num::NonZeroU32;
use core::ptr::NonNull;

use hisi_rf_rtos_driver::{Semaphore, WaitOutcome, WaitTimeout};
use portable_atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use ws63_radio_sys::supplicant::{
    ABI_VERSION, AssociateRequest, AssociateResult, Context, DisconnectEvent, DriverHooks, Event,
    ExternalAuthEvent, ExternalAuthStatus, Key, MAX_SCAN_FREQUENCIES, MAX_SCAN_IE_LEN,
    NativeScanResult, NetworkConfig, OsHooks, Pmf, PollResult, SaePwe, ScanRequest, Security,
    cipher, hisi_wpa_configure, hisi_wpa_connect, hisi_wpa_context_align,
    hisi_wpa_context_diagnostic_word, hisi_wpa_context_size, hisi_wpa_create, hisi_wpa_destroy,
    hisi_wpa_disconnect, hisi_wpa_driver_diagnostic_word, hisi_wpa_driver_install,
    hisi_wpa_eloop_diagnostic_flags, hisi_wpa_event_ring_diagnostic_word,
    hisi_wpa_feed_associate_result, hisi_wpa_feed_disconnect, hisi_wpa_feed_eapol,
    hisi_wpa_feed_external_auth, hisi_wpa_feed_mgmt, hisi_wpa_feed_scan_done,
    hisi_wpa_feed_scan_result, hisi_wpa_init, hisi_wpa_next_event, hisi_wpa_os_install,
    hisi_wpa_os_uninstall, hisi_wpa_poll, hisi_wpa_recovery_diagnostic_word, key_flag,
};

static RUNNER_WAKE: Semaphore = Semaphore::new(0);
static PORT_IDENTITY: u8 = 0;
static DRIVER_CONTEXT: DriverContext = DriverContext::new();
static PORT_STATE: AtomicU8 = AtomicU8::new(PORT_FREE);
static EAPOL_PENDING: AtomicBool = AtomicBool::new(false);
static MGMT_RX_QUEUE: MgmtRxQueue = MgmtRxQueue::new();
static NATIVE_SCAN_ACTIVE: AtomicBool = AtomicBool::new(false);
static SCAN_EVENT_QUEUE: ScanEventQueue = ScanEventQueue::new();
static LINK_EVENT_QUEUE: LinkEventQueue = LinkEventQueue::new();
static EXTERNAL_AUTH_QUEUE: ExternalAuthQueue = ExternalAuthQueue::new();
static DIAG_SCAN_STARTS: AtomicU32 = AtomicU32::new(0);
static DIAG_SCAN_RESULTS: AtomicU32 = AtomicU32::new(0);
static DIAG_SCAN_DONE: AtomicU32 = AtomicU32::new(0);
static DIAG_ASSOCIATE_CALLS: AtomicU32 = AtomicU32::new(0);
static DIAG_ASSOCIATE_RETRIES: AtomicU32 = AtomicU32::new(0);
static DIAG_ASSOCIATE_EVENTS: AtomicU32 = AtomicU32::new(0);
static DIAG_MGMT_EVENTS: AtomicU32 = AtomicU32::new(0);
static DIAG_EAPOL_EVENTS: AtomicU32 = AtomicU32::new(0);
static DIAG_EAPOL_RECEIVE_POLLS: AtomicU32 = AtomicU32::new(0);
static DIAG_EAPOL_RECEIVED: AtomicU32 = AtomicU32::new(0);
static DIAG_EAPOL_FED: AtomicU32 = AtomicU32::new(0);
static DIAG_EAPOL_SENDS: AtomicU32 = AtomicU32::new(0);
static DIAG_KEY_INSTALLS: AtomicU32 = AtomicU32::new(0);
static DIAG_EAPOL_FALLBACK_POLLS: AtomicU32 = AtomicU32::new(0);
static DIAG_EAPOL_FALLBACK_HITS: AtomicU32 = AtomicU32::new(0);
static DIAG_EVENT_RING: AtomicU32 = AtomicU32::new(0);
static DIAG_RECOVERY: AtomicU32 = AtomicU32::new(0);
static DIAG_TEMP_REJECT_CLEARS: AtomicU32 = AtomicU32::new(0);
static DIAG_TEMP_REJECT_CLEAR_FAILURES: AtomicU32 = AtomicU32::new(0);
static DIAG_TEMP_REJECT_CLEAR_STATUS: AtomicU32 = AtomicU32::new(0);
static DIAG_LAST_NATIVE_EVENT_KIND: AtomicU32 = AtomicU32::new(0);
static DIAG_LAST_NATIVE_EVENT_STATUS: AtomicU32 = AtomicU32::new(0);
static DIAG_ASSOC_RESULT_RAW_STATUS: AtomicU32 = AtomicU32::new(0);
static DIAG_ASSOC_RESULT_STATUS: AtomicU32 = AtomicU32::new(0);
static DIAG_ASSOC_RESULT_RESPONSE_IE_LEN: AtomicU32 = AtomicU32::new(0);
static DIAG_DRIVER_FLAGS_STATUS: AtomicU32 = AtomicU32::new(u32::MAX);
static DIAG_DRIVER_FLAGS_LO: AtomicU32 = AtomicU32::new(0);
static DIAG_DRIVER_FLAGS_HI: AtomicU32 = AtomicU32::new(0);
static DIAG_ASSOCIATE_STATUS: AtomicU32 = AtomicU32::new(u32::MAX);
static DIAG_ASSOCIATE_AUTH: AtomicU32 = AtomicU32::new(0);
static DIAG_ASSOCIATE_PMF: AtomicU32 = AtomicU32::new(0);
static DIAG_ASSOCIATE_PWE: AtomicU32 = AtomicU32::new(0);
static DIAG_ASSOCIATE_AKM: AtomicU32 = AtomicU32::new(0);
static DIAG_LAST_IOCTL_STATUS: AtomicU32 = AtomicU32::new(0);
static DIAG_EXTERNAL_AUTH_CALLBACKS: AtomicU32 = AtomicU32::new(0);
static DIAG_EXTERNAL_AUTH_REJECTS: AtomicU32 = AtomicU32::new(0);
static DIAG_EXTERNAL_AUTH_LENGTH: AtomicU32 = AtomicU32::new(0);
static DIAG_TX_AUTH: AuthenticationDiagnostic = AuthenticationDiagnostic::new();
static DIAG_RX_AUTH: AuthenticationDiagnostic = AuthenticationDiagnostic::new();
static DIAG_AUTH_EVENT_SEQUENCE: AtomicU32 = AtomicU32::new(1);
static DIAG_EXTERNAL_AUTH_STARTED: AuthenticationProgress = AuthenticationProgress::new();
static DIAG_AUTH_TX_LAST: AuthenticationProgress = AuthenticationProgress::new();
static DIAG_AUTH_RX_LAST: AuthenticationProgress = AuthenticationProgress::new();
static DIAG_EXTERNAL_AUTH_STATUS_SENT: AuthenticationProgress = AuthenticationProgress::new();
static DIAG_ASSOCIATION_EVENT: AuthenticationProgress = AuthenticationProgress::new();
static DIAG_ASSOCIATION_SEQUENCE: AtomicU32 = AtomicU32::new(1);
static DIAG_ASSOCIATION_ATTEMPTS: [AssociationAttempt; ASSOCIATION_ATTEMPT_CAPACITY] =
    [const { AssociationAttempt::new() }; ASSOCIATION_ATTEMPT_CAPACITY];

const PORT_FREE: u8 = 0;
const PORT_INSTALLING: u8 = 1;
const PORT_READY: u8 = 2;
const PORT_POISONED: u8 = 3;

const ETHERNET_HEADER_LEN: usize = 14;
const EAPOL_ETHERTYPE: [u8; 2] = [0x88, 0x8e];
// The Personal-only vendor profile uses EAPOL_PKT_BUF_SIZE=800. Enterprise is
// a separate future profile and must raise this bound explicitly.
const MAX_EAPOL_PAYLOAD_LEN: usize = 800;
const MAX_EAPOL_RX_FRAME_LEN: usize = 800;
const MAX_MGMT_FRAME_LEN: usize = 768;
const MGMT_RX_QUEUE_DEPTH: usize = 8;
const IFNAME_CAPACITY: usize = 17;

const IOCTL_NEW_KEY: u32 = 1;
const IOCTL_DEL_KEY: u32 = 2;
const IOCTL_SET_KEY: u32 = 3;
const IOCTL_SEND_MLME: u32 = 4;
const IOCTL_SEND_EAPOL: u32 = 5;
const IOCTL_RECEIVE_EAPOL: u32 = 6;
const IOCTL_ENABLE_EAPOL: u32 = 7;
const IOCTL_DISABLE_EAPOL: u32 = 8;
const IOCTL_GET_ADDRESS: u32 = 9;
const IOCTL_SCAN: u32 = 14;
const IOCTL_DISCONNECT: u32 = 15;
const IOCTL_ASSOCIATE: u32 = 16;
const IOCTL_GET_DRIVER_FLAGS: u32 = 35;
const IOCTL_SEND_EXTERNAL_AUTH_STATUS: u32 = 38;
const WLAN_REASON_PREV_AUTH_NOT_VALID: u16 = 2;

const SLOT_FREE: u8 = 0;
const SLOT_WRITING: u8 = 1;
const SLOT_READY: u8 = 2;
const SLOT_READING: u8 = 3;
const SCAN_EVENT_RESULT: u8 = 1;
const SCAN_EVENT_DONE: u8 = 2;
const LINK_EVENT_ASSOCIATE: u8 = 1;
const LINK_EVENT_DISCONNECT: u8 = 2;
const EXTERNAL_AUTH_QUEUE_DEPTH: usize = 2;

// The vendor worker emits a complete scan batch without yielding. Match the
// hostap driver cache (32 BSS entries) plus the terminal scan-done event so the
// callback boundary remains lossless even under a cooperative scheduler.
const SCAN_EVENT_QUEUE_DEPTH: usize = 33;
const LINK_EVENT_QUEUE_DEPTH: usize = 4;
const MAX_ASSOCIATION_IE_LEN: usize = 768;
const VENDOR_ASSOC_STATUS_OFFSET: u16 = 8_000;
const MAX_STANDARD_IEEE_STATUS: u16 = u8::MAX as u16;
const WLAN_STATUS_ASSOC_REJECTED_TEMPORARILY: u16 = 30;
const ASSOCIATION_ATTEMPT_CAPACITY: usize = 8;
const EAPOL_FALLBACK_POLL_WINDOW: u16 = 2_000;
const WLAN_EID_TIMEOUT_INTERVAL: u8 = 56;
const WLAN_TIMEOUT_ASSOC_COMEBACK: u8 = 3;
const NO_COMEBACK_INTERVAL: u32 = u32::MAX;
const VENDOR_AUTH_STATUS_OFFSET: u16 = 7_000;
const WLAN_STATUS_AUTH_TIMEOUT: u16 = 16;
const WLAN_STATUS_INVALID_PMKID: u16 = 53;

const fn vendor_result_requires_stale_state_clear(raw_status: u16) -> bool {
    raw_status == VENDOR_ASSOC_STATUS_OFFSET + WLAN_STATUS_ASSOC_REJECTED_TEMPORARILY
}

const fn normalize_vendor_association_status(status: u16) -> u16 {
    match status.checked_sub(VENDOR_ASSOC_STATUS_OFFSET) {
        Some(standard) if standard != 0 && standard <= MAX_STANDARD_IEEE_STATUS => standard,
        _ => status,
    }
}

/// One non-secret association-result entry retained by the HIL diagnostic ring.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AssociationAttemptDiagnostic {
    pub sequence: u32,
    pub timestamp_ms: u32,
    pub raw_status: u32,
    pub status: u32,
    pub response_ie_len: u32,
    /// Association comeback interval in TUs, or `u32::MAX` when absent.
    pub comeback_tu: u32,
}

struct AssociationAttempt {
    sequence: AtomicU32,
    timestamp_ms: AtomicU32,
    raw_status: AtomicU32,
    status: AtomicU32,
    response_ie_len: AtomicU32,
    comeback_tu: AtomicU32,
}

impl AssociationAttempt {
    const fn new() -> Self {
        Self {
            sequence: AtomicU32::new(0),
            timestamp_ms: AtomicU32::new(0),
            raw_status: AtomicU32::new(0),
            status: AtomicU32::new(0),
            response_ie_len: AtomicU32::new(0),
            comeback_tu: AtomicU32::new(NO_COMEBACK_INTERVAL),
        }
    }

    fn record(&self, sequence: u32, raw_status: u16, status: u16, response_ies: &[u8]) {
        self.timestamp_ms
            .store(crate::uapi::monotonic_ms() as u32, Ordering::Relaxed);
        self.raw_status
            .store(u32::from(raw_status), Ordering::Relaxed);
        self.status.store(u32::from(status), Ordering::Relaxed);
        self.response_ie_len
            .store(response_ies.len() as u32, Ordering::Relaxed);
        self.comeback_tu.store(
            association_comeback_interval(response_ies).unwrap_or(NO_COMEBACK_INTERVAL),
            Ordering::Relaxed,
        );
        self.sequence.store(sequence, Ordering::Release);
    }

    fn snapshot(&self) -> AssociationAttemptDiagnostic {
        let sequence = self.sequence.load(Ordering::Acquire);
        AssociationAttemptDiagnostic {
            sequence,
            timestamp_ms: self.timestamp_ms.load(Ordering::Relaxed),
            raw_status: self.raw_status.load(Ordering::Relaxed),
            status: self.status.load(Ordering::Relaxed),
            response_ie_len: self.response_ie_len.load(Ordering::Relaxed),
            comeback_tu: self.comeback_tu.load(Ordering::Relaxed),
        }
    }
}

fn association_comeback_interval(mut ies: &[u8]) -> Option<u32> {
    while ies.len() >= 2 {
        let length = ies[1] as usize;
        let total = length.checked_add(2)?;
        if total > ies.len() {
            return None;
        }
        if ies[0] == WLAN_EID_TIMEOUT_INTERVAL
            && length == 5
            && ies[2] == WLAN_TIMEOUT_ASSOC_COMEBACK
        {
            return Some(u32::from_le_bytes([ies[3], ies[4], ies[5], ies[6]]));
        }
        ies = &ies[total..];
    }
    None
}

const fn vendor_result_uses_association_reject(raw_status: u16) -> bool {
    raw_status == VENDOR_ASSOC_STATUS_OFFSET + WLAN_STATUS_INVALID_PMKID
        || raw_status == VENDOR_AUTH_STATUS_OFFSET + WLAN_STATUS_AUTH_TIMEOUT
}

const fn vendor_result_uses_disconnect(raw_status: u16) -> bool {
    raw_status != 0 && !vendor_result_uses_association_reject(raw_status)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AuthenticationHeader {
    algorithm: u16,
    transaction: u16,
    status: u16,
}

struct AuthenticationDiagnostic {
    count: AtomicU32,
    frequency: AtomicU32,
    length: AtomicU32,
    algorithm: AtomicU32,
    transaction: AtomicU32,
    status: AtomicU32,
}

struct AuthenticationProgress {
    sequence: AtomicU32,
    timestamp_ms: AtomicU32,
}

impl AuthenticationProgress {
    const fn new() -> Self {
        Self {
            sequence: AtomicU32::new(0),
            timestamp_ms: AtomicU32::new(0),
        }
    }

    fn observe(&self) {
        let sequence = DIAG_AUTH_EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        self.timestamp_ms
            .store(crate::uapi::monotonic_ms() as u32, Ordering::Relaxed);
        self.sequence.store(sequence, Ordering::Release);
    }

    fn snapshot(&self) -> [u32; 2] {
        [
            self.sequence.load(Ordering::Acquire),
            self.timestamp_ms.load(Ordering::Relaxed),
        ]
    }
}

impl AuthenticationDiagnostic {
    const fn new() -> Self {
        Self {
            count: AtomicU32::new(0),
            frequency: AtomicU32::new(0),
            length: AtomicU32::new(0),
            algorithm: AtomicU32::new(0),
            transaction: AtomicU32::new(0),
            status: AtomicU32::new(0),
        }
    }

    fn observe(&self, frame: &[u8], frequency_mhz: u32) -> bool {
        let Some(header) = authentication_header(frame) else {
            return false;
        };
        self.frequency.store(frequency_mhz, Ordering::Relaxed);
        self.length.store(frame.len() as u32, Ordering::Relaxed);
        self.algorithm
            .store(u32::from(header.algorithm), Ordering::Relaxed);
        self.transaction
            .store(u32::from(header.transaction), Ordering::Relaxed);
        self.status
            .store(u32::from(header.status), Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Release);
        true
    }

    fn snapshot(&self) -> [u32; 6] {
        [
            self.count.load(Ordering::Acquire),
            self.frequency.load(Ordering::Relaxed),
            self.length.load(Ordering::Relaxed),
            self.algorithm.load(Ordering::Relaxed),
            self.transaction.load(Ordering::Relaxed),
            self.status.load(Ordering::Relaxed),
        ]
    }
}

fn authentication_header(frame: &[u8]) -> Option<AuthenticationHeader> {
    const AUTHENTICATION_HEADER_LEN: usize = 30;
    const AUTHENTICATION_FRAME_CONTROL: u8 = 0xb0;

    if frame.len() < AUTHENTICATION_HEADER_LEN || frame[0] != AUTHENTICATION_FRAME_CONTROL {
        return None;
    }
    Some(AuthenticationHeader {
        algorithm: u16::from_le_bytes([frame[24], frame[25]]),
        transaction: u16::from_le_bytes([frame[26], frame[27]]),
        status: u16::from_le_bytes([frame[28], frame[29]]),
    })
}

#[derive(Clone, Copy)]
struct MgmtMeta {
    frequency_mhz: u32,
    rssi_dbm: i32,
    frame_len: usize,
}

struct MgmtSlot {
    state: AtomicU8,
    sequence: AtomicU32,
    meta: UnsafeCell<MgmtMeta>,
    frame: UnsafeCell<[u8; MAX_MGMT_FRAME_LEN]>,
}

// SAFETY: ownership of both UnsafeCell values is transferred through the slot
// state with Acquire/Release ordering. WRITING and READING are exclusive.
unsafe impl Sync for MgmtSlot {}

impl MgmtSlot {
    const fn new() -> Self {
        Self {
            state: AtomicU8::new(SLOT_FREE),
            sequence: AtomicU32::new(0),
            meta: UnsafeCell::new(MgmtMeta {
                frequency_mhz: 0,
                rssi_dbm: 0,
                frame_len: 0,
            }),
            frame: UnsafeCell::new([0; MAX_MGMT_FRAME_LEN]),
        }
    }
}

struct MgmtRxQueue {
    next_sequence: AtomicU32,
    dropped: AtomicU32,
    slots: [MgmtSlot; MGMT_RX_QUEUE_DEPTH],
}

impl MgmtRxQueue {
    const fn new() -> Self {
        Self {
            next_sequence: AtomicU32::new(0),
            dropped: AtomicU32::new(0),
            slots: [const { MgmtSlot::new() }; MGMT_RX_QUEUE_DEPTH],
        }
    }

    fn enqueue(&self, frequency_mhz: u32, rssi_dbm: i32, frame: &[u8]) -> bool {
        if frame.is_empty() || frame.len() > MAX_MGMT_FRAME_LEN {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        // Allocate ordering before claiming storage so a nested producer cannot
        // overtake an earlier callback. Gaps from a full queue are harmless.
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        for slot in &self.slots {
            if slot
                .state
                .compare_exchange(SLOT_FREE, SLOT_WRITING, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                continue;
            }
            // SAFETY: WRITING gives this producer exclusive ownership. The
            // potentially large copy intentionally happens outside a critical
            // section; READY publishes all writes to the runner.
            unsafe {
                slot.meta.get().write(MgmtMeta {
                    frequency_mhz,
                    rssi_dbm,
                    frame_len: frame.len(),
                });
                (&mut *slot.frame.get())[..frame.len()].copy_from_slice(frame);
            }
            slot.sequence.store(sequence, Ordering::Relaxed);
            slot.state.store(SLOT_READY, Ordering::Release);
            return true;
        }
        self.dropped.fetch_add(1, Ordering::Relaxed);
        false
    }

    fn take_oldest(&self) -> Option<MgmtFrame<'_>> {
        // Do not overtake a producer that already owns an earlier sequence.
        if self
            .slots
            .iter()
            .any(|slot| slot.state.load(Ordering::Acquire) == SLOT_WRITING)
        {
            return None;
        }
        let mut oldest: Option<(usize, u32)> = None;
        for (index, slot) in self.slots.iter().enumerate() {
            if slot.state.load(Ordering::Acquire) != SLOT_READY {
                continue;
            }
            let sequence = slot.sequence.load(Ordering::Relaxed);
            if oldest.is_none_or(|(_, current)| sequence_before(sequence, current)) {
                oldest = Some((index, sequence));
            }
        }
        let (index, _) = oldest?;
        let slot = &self.slots[index];
        slot.state
            .compare_exchange(
                SLOT_READY,
                SLOT_READING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .ok()?;
        Some(MgmtFrame { slot })
    }

    fn has_pending(&self) -> bool {
        self.slots
            .iter()
            .any(|slot| slot.state.load(Ordering::Acquire) != SLOT_FREE)
    }
}

const fn sequence_before(candidate: u32, current: u32) -> bool {
    (candidate.wrapping_sub(current) as i32) < 0
}

struct MgmtFrame<'a> {
    slot: &'a MgmtSlot,
}

#[derive(Clone, Copy)]
struct ScanMeta {
    kind: u8,
    capabilities: u16,
    flags: u32,
    bssid: [u8; 6],
    frequency_mhz: i32,
    beacon_interval: u16,
    quality: i32,
    level_mbm: i32,
    age_ms: u32,
    ie_len: usize,
    beacon_ie_len: usize,
    status: i32,
}

struct ScanSlot {
    state: AtomicU8,
    sequence: AtomicU32,
    meta: UnsafeCell<ScanMeta>,
    ies: UnsafeCell<[u8; MAX_SCAN_IE_LEN]>,
}

// SAFETY: WRITING/READING are exclusive ownership states and READY publishes
// the complete deep copy with Release ordering.
unsafe impl Sync for ScanSlot {}

impl ScanSlot {
    const fn new() -> Self {
        Self {
            state: AtomicU8::new(SLOT_FREE),
            sequence: AtomicU32::new(0),
            meta: UnsafeCell::new(ScanMeta {
                kind: 0,
                capabilities: 0,
                flags: 0,
                bssid: [0; 6],
                frequency_mhz: 0,
                beacon_interval: 0,
                quality: 0,
                level_mbm: 0,
                age_ms: 0,
                ie_len: 0,
                beacon_ie_len: 0,
                status: 0,
            }),
            ies: UnsafeCell::new([0; MAX_SCAN_IE_LEN]),
        }
    }
}

struct ScanEventQueue {
    next_sequence: AtomicU32,
    dropped: AtomicU32,
    accepted_results: AtomicU32,
    truncated_results: AtomicU32,
    slots: [ScanSlot; SCAN_EVENT_QUEUE_DEPTH],
}

impl ScanEventQueue {
    const fn new() -> Self {
        Self {
            next_sequence: AtomicU32::new(0),
            dropped: AtomicU32::new(0),
            accepted_results: AtomicU32::new(0),
            truncated_results: AtomicU32::new(0),
            slots: [const { ScanSlot::new() }; SCAN_EVENT_QUEUE_DEPTH],
        }
    }

    fn begin_transaction(&self) {
        self.accepted_results.store(0, Ordering::Release);
    }

    fn enqueue_result(&self, meta: ScanMeta, ies: &[u8]) -> bool {
        if meta.kind != SCAN_EVENT_RESULT
            || meta.ie_len.saturating_add(meta.beacon_ie_len) != ies.len()
            || ies.len() > MAX_SCAN_IE_LEN
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        if self
            .accepted_results
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < crate::wifi::MAX_SCAN_RESULTS as u32).then_some(count + 1)
            })
            .is_err()
        {
            // The C driver cache intentionally exposes at most MAX_SCAN_RESULTS
            // BSS entries. Preserve the terminal scan-done slot and report this
            // as bounded truncation, not as a queue failure that can leak into a
            // later connect transaction.
            self.truncated_results.fetch_add(1, Ordering::Relaxed);
            return true;
        }
        self.enqueue(meta, ies)
    }

    fn enqueue_done(&self, status: i32) -> bool {
        self.enqueue(
            ScanMeta {
                kind: SCAN_EVENT_DONE,
                capabilities: 0,
                flags: 0,
                bssid: [0; 6],
                frequency_mhz: 0,
                beacon_interval: 0,
                quality: 0,
                level_mbm: 0,
                age_ms: 0,
                ie_len: 0,
                beacon_ie_len: 0,
                status,
            },
            &[],
        )
    }

    fn enqueue(&self, meta: ScanMeta, ies: &[u8]) -> bool {
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        for slot in &self.slots {
            if slot
                .state
                .compare_exchange(SLOT_FREE, SLOT_WRITING, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                continue;
            }
            // SAFETY: WRITING exclusively owns both cells. The large copy is
            // deliberately outside any interrupt-disabled critical section.
            unsafe {
                slot.meta.get().write(meta);
                (&mut *slot.ies.get())[..ies.len()].copy_from_slice(ies);
            }
            slot.sequence.store(sequence, Ordering::Relaxed);
            slot.state.store(SLOT_READY, Ordering::Release);
            return true;
        }
        self.dropped.fetch_add(1, Ordering::Relaxed);
        false
    }

    fn take_oldest(&self) -> Option<ScanEvent<'_>> {
        take_oldest_slot(&self.slots).map(|slot| ScanEvent { slot })
    }

    fn has_pending(&self) -> bool {
        self.slots
            .iter()
            .any(|slot| slot.state.load(Ordering::Acquire) != SLOT_FREE)
    }

    fn discard_pending(&self) {
        while self.take_oldest().is_some() {}
    }
}

struct ScanEvent<'a> {
    slot: &'a ScanSlot,
}

impl ScanEvent<'_> {
    fn meta(&self) -> ScanMeta {
        // SAFETY: READING owns the initialized metadata.
        unsafe { *self.slot.meta.get() }
    }

    fn ies(&self) -> &[u8] {
        let meta = self.meta();
        let len = meta.ie_len + meta.beacon_ie_len;
        // SAFETY: the producer checked and initialized exactly this prefix.
        unsafe { &(&*self.slot.ies.get())[..len] }
    }
}

impl Drop for ScanEvent<'_> {
    fn drop(&mut self) {
        self.slot.state.store(SLOT_FREE, Ordering::Release);
    }
}

#[derive(Clone, Copy)]
struct LinkMeta {
    kind: u8,
    status_or_reason: u16,
    frequency_mhz: u16,
    bssid: [u8; 6],
    first_len: usize,
    second_len: usize,
}

struct LinkSlot {
    state: AtomicU8,
    sequence: AtomicU32,
    meta: UnsafeCell<LinkMeta>,
    first: UnsafeCell<[u8; MAX_ASSOCIATION_IE_LEN]>,
    second: UnsafeCell<[u8; MAX_ASSOCIATION_IE_LEN]>,
}

// SAFETY: slot state transfers exclusive ownership of all cells.
unsafe impl Sync for LinkSlot {}

impl LinkSlot {
    const fn new() -> Self {
        Self {
            state: AtomicU8::new(SLOT_FREE),
            sequence: AtomicU32::new(0),
            meta: UnsafeCell::new(LinkMeta {
                kind: 0,
                status_or_reason: 0,
                frequency_mhz: 0,
                bssid: [0; 6],
                first_len: 0,
                second_len: 0,
            }),
            first: UnsafeCell::new([0; MAX_ASSOCIATION_IE_LEN]),
            second: UnsafeCell::new([0; MAX_ASSOCIATION_IE_LEN]),
        }
    }
}

struct LinkEventQueue {
    next_sequence: AtomicU32,
    dropped: AtomicU32,
    slots: [LinkSlot; LINK_EVENT_QUEUE_DEPTH],
}

impl LinkEventQueue {
    const fn new() -> Self {
        Self {
            next_sequence: AtomicU32::new(0),
            dropped: AtomicU32::new(0),
            slots: [const { LinkSlot::new() }; LINK_EVENT_QUEUE_DEPTH],
        }
    }

    fn enqueue(&self, meta: LinkMeta, first: &[u8], second: &[u8]) -> bool {
        if first.len() != meta.first_len
            || second.len() != meta.second_len
            || first.len() > MAX_ASSOCIATION_IE_LEN
            || second.len() > MAX_ASSOCIATION_IE_LEN
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        for slot in &self.slots {
            if slot
                .state
                .compare_exchange(SLOT_FREE, SLOT_WRITING, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                continue;
            }
            // SAFETY: WRITING exclusively owns all payload cells until READY.
            unsafe {
                slot.meta.get().write(meta);
                (&mut *slot.first.get())[..first.len()].copy_from_slice(first);
                (&mut *slot.second.get())[..second.len()].copy_from_slice(second);
            }
            slot.sequence.store(sequence, Ordering::Relaxed);
            slot.state.store(SLOT_READY, Ordering::Release);
            return true;
        }
        self.dropped.fetch_add(1, Ordering::Relaxed);
        false
    }

    fn take_oldest(&self) -> Option<LinkEvent<'_>> {
        take_oldest_slot(&self.slots).map(|slot| LinkEvent { slot })
    }

    fn has_pending(&self) -> bool {
        self.slots
            .iter()
            .any(|slot| slot.state.load(Ordering::Acquire) != SLOT_FREE)
    }
}

struct LinkEvent<'a> {
    slot: &'a LinkSlot,
}

impl LinkEvent<'_> {
    fn meta(&self) -> LinkMeta {
        // SAFETY: READING owns the initialized metadata.
        unsafe { *self.slot.meta.get() }
    }

    fn first(&self) -> &[u8] {
        let len = self.meta().first_len;
        // SAFETY: the producer initialized this checked prefix.
        unsafe { &(&*self.slot.first.get())[..len] }
    }

    fn second(&self) -> &[u8] {
        let len = self.meta().second_len;
        // SAFETY: the producer initialized this checked prefix.
        unsafe { &(&*self.slot.second.get())[..len] }
    }
}

impl Drop for LinkEvent<'_> {
    fn drop(&mut self) {
        self.slot.state.store(SLOT_FREE, Ordering::Release);
    }
}

struct ExternalAuthSlot {
    state: AtomicU8,
    sequence: AtomicU32,
    event: UnsafeCell<ExternalAuthEvent>,
}

// SAFETY: slot state transfers exclusive ownership of the event cell from the
// vendor callback to the single RadioRunner.
unsafe impl Sync for ExternalAuthSlot {}

impl ExternalAuthSlot {
    const fn new() -> Self {
        Self {
            state: AtomicU8::new(SLOT_FREE),
            sequence: AtomicU32::new(0),
            event: UnsafeCell::new(ExternalAuthEvent {
                abi_version: ABI_VERSION,
                action: 0,
                ssid_len: 0,
                bssid: [0; 6],
                status: 0,
                key_mgmt_suite: 0,
                pmkid_present: 0,
                reserved: [0; 3],
                ssid: [0; 32],
                pmkid: [0; 16],
            }),
        }
    }
}

struct ExternalAuthQueue {
    next_sequence: AtomicU32,
    dropped: AtomicU32,
    slots: [ExternalAuthSlot; EXTERNAL_AUTH_QUEUE_DEPTH],
}

impl ExternalAuthQueue {
    const fn new() -> Self {
        Self {
            next_sequence: AtomicU32::new(0),
            dropped: AtomicU32::new(0),
            slots: [const { ExternalAuthSlot::new() }; EXTERNAL_AUTH_QUEUE_DEPTH],
        }
    }

    fn enqueue(&self, event: ExternalAuthEvent) -> bool {
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        for slot in &self.slots {
            if slot
                .state
                .compare_exchange(SLOT_FREE, SLOT_WRITING, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                continue;
            }
            // SAFETY: WRITING exclusively owns the cell until READY publishes
            // the complete fixed-size deep copy.
            unsafe { slot.event.get().write(event) };
            slot.sequence.store(sequence, Ordering::Relaxed);
            slot.state.store(SLOT_READY, Ordering::Release);
            return true;
        }
        self.dropped.fetch_add(1, Ordering::Relaxed);
        false
    }

    fn take_oldest(&self) -> Option<ExternalAuthGuard<'_>> {
        take_oldest_slot(&self.slots).map(|slot| ExternalAuthGuard { slot })
    }

    fn has_pending(&self) -> bool {
        self.slots
            .iter()
            .any(|slot| slot.state.load(Ordering::Acquire) != SLOT_FREE)
    }
}

struct ExternalAuthGuard<'a> {
    slot: &'a ExternalAuthSlot,
}

impl ExternalAuthGuard<'_> {
    fn event(&self) -> ExternalAuthEvent {
        // SAFETY: READING owns the initialized fixed-size event.
        unsafe { *self.slot.event.get() }
    }
}

impl Drop for ExternalAuthGuard<'_> {
    fn drop(&mut self) {
        self.slot.state.store(SLOT_FREE, Ordering::Release);
    }
}

trait SequencedSlot {
    fn state(&self) -> &AtomicU8;
    fn sequence(&self) -> &AtomicU32;
}

impl SequencedSlot for ScanSlot {
    fn state(&self) -> &AtomicU8 {
        &self.state
    }
    fn sequence(&self) -> &AtomicU32 {
        &self.sequence
    }
}

impl SequencedSlot for LinkSlot {
    fn state(&self) -> &AtomicU8 {
        &self.state
    }
    fn sequence(&self) -> &AtomicU32 {
        &self.sequence
    }
}

impl SequencedSlot for ExternalAuthSlot {
    fn state(&self) -> &AtomicU8 {
        &self.state
    }
    fn sequence(&self) -> &AtomicU32 {
        &self.sequence
    }
}

fn take_oldest_slot<T: SequencedSlot>(slots: &[T]) -> Option<&T> {
    if slots
        .iter()
        .any(|slot| slot.state().load(Ordering::Acquire) == SLOT_WRITING)
    {
        return None;
    }
    let mut oldest: Option<(usize, u32)> = None;
    for (index, slot) in slots.iter().enumerate() {
        if slot.state().load(Ordering::Acquire) != SLOT_READY {
            continue;
        }
        let sequence = slot.sequence().load(Ordering::Relaxed);
        if oldest.is_none_or(|(_, current)| sequence_before(sequence, current)) {
            oldest = Some((index, sequence));
        }
    }
    let slot = &slots[oldest?.0];
    slot.state()
        .compare_exchange(
            SLOT_READY,
            SLOT_READING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .ok()?;
    Some(slot)
}

impl MgmtFrame<'_> {
    fn meta(&self) -> MgmtMeta {
        // SAFETY: READING gives this guard exclusive immutable access and the
        // Acquire transition observed the producer's complete metadata.
        unsafe { *self.slot.meta.get() }
    }

    fn bytes(&self) -> &[u8] {
        let len = self.meta().frame_len;
        // SAFETY: the producer validated len and published the initialized
        // prefix before READY. This slot cannot be reused until Drop.
        unsafe { &(&*self.slot.frame.get())[..len] }
    }
}

impl Drop for MgmtFrame<'_> {
    fn drop(&mut self) {
        self.slot.state.store(SLOT_FREE, Ordering::Release);
    }
}

const KEY_TYPE_GROUP: i32 = 0;
const KEY_TYPE_PAIRWISE: i32 = 1;
const KEY_DEFAULT_INVALID: u8 = 0;
const KEY_DEFAULT_UNICAST: u8 = 1;
const KEY_DEFAULT_MULTICAST: u8 = 2;

struct DriverContext {
    ifname: UnsafeCell<[u8; IFNAME_CAPACITY]>,
    send_action_cookie: UnsafeCell<u64>,
}

// SAFETY: the interface name is written only while PORT_INSTALLING and is
// published by PORT_READY. Hostap serializes driver control calls through its
// single RadioRunner; only the management callback mutates the cookie.
unsafe impl Sync for DriverContext {}

impl DriverContext {
    const fn new() -> Self {
        Self {
            ifname: UnsafeCell::new([0; IFNAME_CAPACITY]),
            send_action_cookie: UnsafeCell::new(0),
        }
    }

    fn initialize(&self, ifname: &[u8]) -> bool {
        if ifname.is_empty() || ifname.len() >= IFNAME_CAPACITY || ifname.contains(&0) {
            return false;
        }
        let mut stored = [0; IFNAME_CAPACITY];
        stored[..ifname.len()].copy_from_slice(ifname);
        // SAFETY: the caller owns the PORT_INSTALLING state; no callback can
        // observe the context before driver registration and PORT_READY.
        unsafe {
            self.ifname.get().write(stored);
            self.send_action_cookie.get().write(0);
        }
        true
    }

    fn matches(&self, ifname: &[u8]) -> bool {
        // SAFETY: PORT_READY publishes the initialized immutable name.
        let stored = unsafe { &*self.ifname.get() };
        stored
            .iter()
            .position(|byte| *byte == 0)
            .is_some_and(|len| &stored[..len] == ifname)
    }

    fn ifname(&self) -> &[u8; IFNAME_CAPACITY] {
        // SAFETY: callbacks are installed only after initialization and the
        // name remains immutable for the firmware lifetime.
        unsafe { &*self.ifname.get() }
    }
}

#[repr(C)]
struct TxEapol {
    buffer: *mut u8,
    length: u32,
}

#[repr(C)]
struct RxEapol {
    buffer: *mut u8,
    length: u32,
}

type EapolNotify = unsafe extern "C" fn(*mut c_void, *mut c_void);

#[repr(C)]
struct EnableEapol {
    callback: Option<EapolNotify>,
    context: *mut c_void,
}

#[repr(C)]
struct MlmeData {
    frequency_mhz: u32,
    data_len: u32,
    data: *mut u8,
    send_action_cookie: *mut u64,
}

#[repr(C)]
struct VendorExternalAuthStatus {
    action: u8,
    bssid: [u8; 6],
    reserved0: u8,
    ssid: *mut u8,
    ssid_len: u32,
    key_mgmt_suite: u32,
    status: u16,
    reserved1: [u8; 2],
    pmkid: *mut u8,
}

#[repr(C)]
struct KeyExtension {
    key_type: i32,
    key_index: u32,
    key_len: u32,
    sequence_len: u32,
    cipher: u32,
    address: *mut u8,
    material: *mut u8,
    sequence: *mut u8,
    default_data: u8,
    default_management: u8,
    default_types: u8,
    reserved: u8,
}

#[repr(C)]
struct VendorScanSsid {
    ssid: [u8; 32],
    ssid_len: u32,
}

#[repr(C)]
struct VendorScanRequest {
    ssids: *mut VendorScanSsid,
    frequencies: *mut i32,
    extra_ies: *mut u8,
    bssid: *mut u8,
    num_ssids: u8,
    num_frequencies: u8,
    prefix_ssid_scan: u8,
    fast_connect: u8,
    extra_ies_len: u32,
    acs_scan: u32,
}

#[repr(C)]
struct VendorCryptoSettings {
    wpa_versions: u32,
    cipher_group: u32,
    num_pairwise: i32,
    pairwise: [u32; 5],
    num_akm: i32,
    akm: [u32; 2],
    sae_pwe: i32,
}

#[repr(C)]
struct VendorAssociateRequest {
    bssid: *mut u8,
    ssid: *mut u8,
    ies: *mut u8,
    key: *mut u8,
    auth_type: u8,
    privacy: u8,
    key_len: u8,
    key_index: u8,
    pmf: u8,
    auto_connect: u8,
    reserved: [u8; 2],
    frequency_mhz: u32,
    ssid_len: u32,
    ies_len: u32,
    crypto: *mut VendorCryptoSettings,
}

#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(core::mem::size_of::<TxEapol>() == 8);
    assert!(core::mem::size_of::<RxEapol>() == 8);
    assert!(core::mem::size_of::<EnableEapol>() == 8);
    assert!(core::mem::size_of::<MlmeData>() == 16);
    assert!(core::mem::size_of::<VendorExternalAuthStatus>() == 28);
    assert!(core::mem::offset_of!(VendorExternalAuthStatus, bssid) == 1);
    assert!(core::mem::offset_of!(VendorExternalAuthStatus, ssid) == 8);
    assert!(core::mem::offset_of!(VendorExternalAuthStatus, pmkid) == 24);
    assert!(core::mem::size_of::<KeyExtension>() == 36);
    assert!(core::mem::offset_of!(KeyExtension, address) == 20);
    assert!(core::mem::offset_of!(KeyExtension, default_data) == 32);
    assert!(core::mem::size_of::<VendorScanSsid>() == 36);
    assert!(core::mem::size_of::<VendorScanRequest>() == 28);
    assert!(core::mem::offset_of!(VendorScanRequest, frequencies) == 4);
    assert!(core::mem::offset_of!(VendorScanRequest, num_ssids) == 16);
    assert!(core::mem::offset_of!(VendorScanRequest, extra_ies_len) == 20);
    assert!(core::mem::size_of::<VendorCryptoSettings>() == 48);
    assert!(core::mem::offset_of!(VendorCryptoSettings, pairwise) == 12);
    assert!(core::mem::offset_of!(VendorCryptoSettings, num_akm) == 32);
    assert!(core::mem::offset_of!(VendorCryptoSettings, akm) == 36);
    assert!(core::mem::offset_of!(VendorCryptoSettings, sae_pwe) == 44);
    assert!(core::mem::size_of::<VendorAssociateRequest>() == 40);
    assert!(core::mem::offset_of!(VendorAssociateRequest, auth_type) == 16);
    assert!(core::mem::offset_of!(VendorAssociateRequest, frequency_mhz) == 24);
    assert!(core::mem::offset_of!(VendorAssociateRequest, crypto) == 36);
};

/// Failure while registering the upstream supplicant native runtime seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpstreamSupplicantPortError {
    /// The application has not installed a runtime or exhausted its resources.
    Runtime(hisi_rf_rtos_driver::Error),
    /// The interface name is empty, contains NUL, or exceeds the WS63 ABI.
    InvalidInterfaceName,
    /// Another context is currently installing the singleton port.
    Busy,
    /// A previous failed rollback left the singleton registration uncertain.
    Poisoned,
    /// The singleton is already installed for another interface.
    InterfaceConflict,
    /// The C ABI rejected the hook table or another runtime already owns it.
    Abi(i32),
    /// Driver installation failed and the OS hook rollback also failed.
    Rollback { install: i32, rollback: i32 },
}

/// Failure while owning or advancing the opaque upstream supplicant context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeSupplicantError {
    /// The native OS/driver seam could not be installed.
    Port(UpstreamSupplicantPortError),
    /// The C implementation reported an invalid opaque-context layout.
    InvalidContextLayout,
    /// The RF heap could not provide context storage.
    AllocationFailed,
    /// The C implementation rejected the supplied storage or driver hooks.
    CreateFailed,
    /// Upstream hostap initialization failed.
    InitializeFailed(i32),
    /// The WS63 driver rejected EAPOL receive notification registration.
    EnableEapolFailed(i32),
    /// One queued management frame could not be delivered to hostap.
    FeedMgmtFailed(i32),
    /// One or more management frames could not fit the bounded RX queue.
    MgmtQueueOverflow(u32),
    /// One queued scan event could not be delivered to hostap.
    FeedScanFailed(i32),
    /// One or more scan events could not fit the bounded RX queue.
    ScanQueueOverflow(u32),
    /// One queued association/disconnect event could not be delivered.
    FeedLinkFailed(i32),
    /// One or more association/disconnect events overflowed the queue.
    LinkQueueOverflow(u32),
    /// One queued external-auth event could not be delivered to hostap.
    FeedExternalAuthFailed(i32),
    /// One or more external-auth events overflowed the queue.
    ExternalAuthQueueOverflow(u32),
    /// The WS63 EAPOL receive ioctl or hostap feed rejected a frame.
    FeedEapolFailed(i32),
    /// The typed station configuration was rejected by the native context.
    ConfigureFailed(i32),
    /// Starting association was rejected by the native context.
    ConnectFailed(i32),
    /// Starting deauthentication was rejected by the native context.
    DisconnectFailed(i32),
    /// One event or poll result violated the versioned C ABI.
    InvalidResult,
    /// The upstream event loop reported a runtime failure.
    PollFailed(i32),
}

/// Exclusive owner of one opaque upstream hostap context.
///
/// The owner is intentionally crate-private until W2D closes the complete
/// scan/auth/assoc and RX event path. It is moved into the single radio runner;
/// callbacks only queue data and wake that runner.
#[allow(dead_code)]
pub(crate) struct NativeSupplicant {
    context: NonNull<Context>,
    storage: NonNull<c_void>,
    mgmt_dropped_seen: u32,
    scan_dropped_seen: u32,
    link_dropped_seen: u32,
    external_auth_dropped_seen: u32,
    eapol_fallback_polls_remaining: u16,
}

#[allow(dead_code)]
impl NativeSupplicant {
    pub(crate) fn create(ifname: &[u8]) -> Result<Self, NativeSupplicantError> {
        prepare_upstream_supplicant_port(ifname).map_err(NativeSupplicantError::Port)?;

        // SAFETY: these two pure queries expose the layout of the matching C
        // implementation linked by ws63-radio-sys.
        let (size, alignment) = unsafe { (hisi_wpa_context_size(), hisi_wpa_context_align()) };
        if !valid_context_layout(size, alignment) {
            return Err(NativeSupplicantError::InvalidContextLayout);
        }
        let storage = NonNull::new(crate::alloc::allocate_zeroed(size, alignment))
            .ok_or(NativeSupplicantError::AllocationFailed)?;
        let hooks = driver_hooks();
        // SAFETY: `storage` is live, zeroed and has the exact queried layout;
        // the hook table is copied synchronously by the C boundary.
        let context = unsafe { hisi_wpa_create(storage.as_ptr(), size, &raw const hooks) };
        let Some(context) = NonNull::new(context) else {
            crate::alloc::osal_kfree(storage.as_ptr());
            return Err(NativeSupplicantError::CreateFailed);
        };
        // SAFETY: `context` was returned by hisi_wpa_create and remains owned
        // by this value until Drop.
        let initialized = unsafe { hisi_wpa_init(context.as_ptr()) };
        if initialized != 0 {
            // The C destroy path handles partial wpa_supplicant_init state and
            // releases its driver user before the backing storage is freed.
            unsafe { hisi_wpa_destroy(context.as_ptr()) };
            crate::alloc::osal_kfree(storage.as_ptr());
            return Err(NativeSupplicantError::InitializeFailed(initialized));
        }
        EAPOL_PENDING.store(false, Ordering::Release);
        let mut enable = EnableEapol {
            callback: Some(eapol_notify),
            context: context.as_ptr().cast(),
        };
        let enabled = crate::wal::ioctl(
            DRIVER_CONTEXT.ifname(),
            IOCTL_ENABLE_EAPOL,
            (&mut enable as *mut EnableEapol).cast(),
        );
        if enabled != 0 {
            // SAFETY: context is still exclusively owned and may be destroyed
            // after a failed transport registration.
            unsafe { hisi_wpa_destroy(context.as_ptr()) };
            crate::alloc::osal_kfree(storage.as_ptr());
            return Err(NativeSupplicantError::EnableEapolFailed(enabled));
        }
        Ok(Self {
            context,
            storage,
            mgmt_dropped_seen: MGMT_RX_QUEUE.dropped.load(Ordering::Acquire),
            scan_dropped_seen: SCAN_EVENT_QUEUE.dropped.load(Ordering::Acquire),
            link_dropped_seen: LINK_EVENT_QUEUE.dropped.load(Ordering::Acquire),
            external_auth_dropped_seen: EXTERNAL_AUTH_QUEUE.dropped.load(Ordering::Acquire),
            eapol_fallback_polls_remaining: 0,
        })
    }

    pub(crate) fn configure(
        &mut self,
        config: &hisi_rf::StationConfig,
    ) -> Result<(), NativeSupplicantError> {
        let mut network = NetworkConfig {
            abi_version: ABI_VERSION,
            security: 0,
            pmf: 0,
            ssid_len: config.ssid.as_bytes().len() as u8,
            sae_pwe: 0,
            channel: config.channel,
            reserved0: 0,
            ssid: [0; 32],
            bssid: config.bssid,
            reserved1: [0; 2],
        };
        network.ssid[..config.ssid.as_bytes().len()].copy_from_slice(config.ssid.as_bytes());
        match config.security() {
            hisi_rf::PersonalSecurity::Wpa2 => {
                network.security = Security::Wpa2Psk as u8;
                network.pmf = Pmf::Optional as u8;
            }
            hisi_rf::PersonalSecurity::Wpa3 { sae_pwe } => {
                network.security = Security::Wpa3Sae as u8;
                network.pmf = Pmf::Required as u8;
                network.sae_pwe = match sae_pwe {
                    hisi_rf::SaePwe::HuntAndPeck => SaePwe::HuntAndPeck as u8,
                    hisi_rf::SaePwe::HashToElement => SaePwe::HashToElement as u8,
                    hisi_rf::SaePwe::Both => SaePwe::Both as u8,
                };
            }
        }
        let passphrase = config.passphrase.expose_secret();
        // SAFETY: the unique owner serializes context access; all borrowed
        // config/passphrase bytes remain live for this synchronous call.
        let status = unsafe {
            hisi_wpa_configure(
                self.context.as_ptr(),
                &raw const network,
                passphrase.as_ptr(),
                passphrase.len(),
            )
        };
        (status == 0)
            .then_some(())
            .ok_or(NativeSupplicantError::ConfigureFailed(status))
    }

    pub(crate) fn connect(&mut self) -> Result<(), NativeSupplicantError> {
        // SAFETY: the unique owner serializes all context calls.
        let status = unsafe { hisi_wpa_connect(self.context.as_ptr()) };
        (status == 0)
            .then_some(())
            .ok_or(NativeSupplicantError::ConnectFailed(status))
    }

    pub(crate) fn disconnect(&mut self) -> Result<(), NativeSupplicantError> {
        // SAFETY: the unique owner serializes all context calls.
        let status = unsafe { hisi_wpa_disconnect(self.context.as_ptr()) };
        (status == 0)
            .then_some(())
            .ok_or(NativeSupplicantError::DisconnectFailed(status))
    }

    /// Capture the next vendor scan into hostap's BSS cache.
    ///
    /// The public Wi-Fi controller already scans before selecting a network.
    /// Reusing those exact raw scan events avoids starting a second firmware
    /// scan from `wpa_supplicant_select_network` and keeps one scan transaction
    /// as the source of both the Rust result list and hostap's BSS cache.
    pub(crate) fn begin_scan_cache_capture(&mut self) -> Result<(), NativeSupplicantError> {
        if PORT_STATE.load(Ordering::Acquire) != PORT_READY
            || NATIVE_SCAN_ACTIVE
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return Err(NativeSupplicantError::InvalidResult);
        }
        SCAN_EVENT_QUEUE.begin_transaction();
        Ok(())
    }

    /// Finish a completed scan capture and update hostap's BSS cache.
    pub(crate) fn finish_scan_cache_capture(&mut self) -> Result<(), NativeSupplicantError> {
        if NATIVE_SCAN_ACTIVE.load(Ordering::Acquire) {
            return Err(NativeSupplicantError::InvalidResult);
        }
        while SCAN_EVENT_QUEUE.has_pending() {
            self.poll(NonZeroU32::new(64).unwrap())?;
        }
        Ok(())
    }

    /// Abort a failed vendor scan without leaking events into the next scan.
    pub(crate) fn cancel_scan_cache_capture(&mut self) {
        NATIVE_SCAN_ACTIVE.store(false, Ordering::Release);
        SCAN_EVENT_QUEUE.discard_pending();
    }

    pub(crate) fn context_diagnostic_word(&self) -> u32 {
        // SAFETY: the unique owner keeps the native context alive and
        // serializes this read-only snapshot with all other context calls.
        unsafe { hisi_wpa_context_diagnostic_word(self.context.as_ptr()) }
    }

    fn event_ring_diagnostic_word(&self) -> u32 {
        // SAFETY: the unique owner keeps the native context alive and
        // serializes this read-only snapshot with all other context calls.
        unsafe { hisi_wpa_event_ring_diagnostic_word(self.context.as_ptr()) }
    }

    /// Advance bounded hostap work from the owning radio runner.
    pub(crate) fn poll(
        &mut self,
        work_budget: NonZeroU32,
    ) -> Result<PollResult, NativeSupplicantError> {
        let dropped = MGMT_RX_QUEUE.dropped.load(Ordering::Acquire);
        if dropped != self.mgmt_dropped_seen {
            let delta = dropped.wrapping_sub(self.mgmt_dropped_seen);
            self.mgmt_dropped_seen = dropped;
            return Err(NativeSupplicantError::MgmtQueueOverflow(delta));
        }
        let dropped = SCAN_EVENT_QUEUE.dropped.load(Ordering::Acquire);
        if dropped != self.scan_dropped_seen {
            let delta = dropped.wrapping_sub(self.scan_dropped_seen);
            self.scan_dropped_seen = dropped;
            return Err(NativeSupplicantError::ScanQueueOverflow(delta));
        }
        let dropped = LINK_EVENT_QUEUE.dropped.load(Ordering::Acquire);
        if dropped != self.link_dropped_seen {
            let delta = dropped.wrapping_sub(self.link_dropped_seen);
            self.link_dropped_seen = dropped;
            return Err(NativeSupplicantError::LinkQueueOverflow(delta));
        }
        let dropped = EXTERNAL_AUTH_QUEUE.dropped.load(Ordering::Acquire);
        if dropped != self.external_auth_dropped_seen {
            let delta = dropped.wrapping_sub(self.external_auth_dropped_seen);
            self.external_auth_dropped_seen = dropped;
            return Err(NativeSupplicantError::ExternalAuthQueueOverflow(delta));
        }
        let mut rx_work = false;
        let mut rx_budget = work_budget.get().saturating_sub(1);
        while rx_budget != 0 {
            let Some(event) = SCAN_EVENT_QUEUE.take_oldest() else {
                break;
            };
            let meta = event.meta();
            let status = if meta.kind == SCAN_EVENT_RESULT {
                let ies = event.ies();
                let result = NativeScanResult {
                    abi_version: ABI_VERSION,
                    capabilities: meta.capabilities,
                    flags: meta.flags,
                    bssid: meta.bssid,
                    reserved0: [0; 2],
                    frequency_mhz: meta.frequency_mhz,
                    beacon_interval: meta.beacon_interval,
                    reserved1: 0,
                    quality: meta.quality,
                    level_mbm: meta.level_mbm,
                    age_ms: meta.age_ms,
                    ie_len: meta.ie_len as u32,
                    beacon_ie_len: meta.beacon_ie_len as u32,
                    ies: ies.as_ptr(),
                };
                // SAFETY: the queue guard keeps the deep-copied IE payload live
                // for this synchronous context call.
                unsafe { hisi_wpa_feed_scan_result(self.context.as_ptr(), &raw const result) }
            } else if meta.kind == SCAN_EVENT_DONE {
                // SAFETY: the unique owner serializes context calls.
                unsafe { hisi_wpa_feed_scan_done(self.context.as_ptr(), meta.status) }
            } else {
                return Err(NativeSupplicantError::InvalidResult);
            };
            if status != 0 {
                return Err(NativeSupplicantError::FeedScanFailed(status));
            }
            rx_work = true;
            rx_budget -= 1;
        }
        while rx_budget != 0 {
            let Some(event) = LINK_EVENT_QUEUE.take_oldest() else {
                break;
            };
            let meta = event.meta();
            let status = if meta.kind == LINK_EVENT_ASSOCIATE {
                let first = event.first();
                let second = event.second();
                let result = AssociateResult {
                    abi_version: ABI_VERSION,
                    status: meta.status_or_reason,
                    frequency_mhz: meta.frequency_mhz,
                    reserved: 0,
                    bssid: meta.bssid,
                    reserved1: [0; 2],
                    request_ies: first.as_ptr(),
                    request_ies_len: first.len(),
                    response_ies: second.as_ptr(),
                    response_ies_len: second.len(),
                };
                // SAFETY: both queue payload guards live for the synchronous call.
                let status = unsafe {
                    hisi_wpa_feed_associate_result(self.context.as_ptr(), &raw const result)
                };
                if status == 0 && meta.status_or_reason == 0 {
                    self.eapol_fallback_polls_remaining = EAPOL_FALLBACK_POLL_WINDOW;
                }
                status
            } else if meta.kind == LINK_EVENT_DISCONNECT {
                if vendor_result_requires_stale_state_clear(meta.status_or_reason) {
                    // The WS63 MAC reports status 8030 after the AP rejects a
                    // stale PMF association. Clear that station state from the
                    // runner before hostap starts its next association cycle.
                    // The vendor callback remains enqueue-only; this bounded
                    // ioctl runs in normal thread context.
                    let mut reason = WLAN_REASON_PREV_AUTH_NOT_VALID;
                    DIAG_TEMP_REJECT_CLEARS.fetch_add(1, Ordering::Relaxed);
                    let clear_status = crate::wal::ioctl(
                        DRIVER_CONTEXT.ifname(),
                        IOCTL_DISCONNECT,
                        (&mut reason as *mut u16).cast(),
                    );
                    DIAG_TEMP_REJECT_CLEAR_STATUS.store(clear_status as u32, Ordering::Release);
                    if clear_status != 0 {
                        DIAG_TEMP_REJECT_CLEAR_FAILURES.fetch_add(1, Ordering::Relaxed);
                        return Err(NativeSupplicantError::DisconnectFailed(clear_status));
                    }
                }
                let ies = event.first();
                let event = DisconnectEvent {
                    abi_version: ABI_VERSION,
                    reason: meta.status_or_reason,
                    ies: ies.as_ptr(),
                    ies_len: ies.len(),
                };
                // SAFETY: the queue payload remains live for the synchronous call.
                unsafe { hisi_wpa_feed_disconnect(self.context.as_ptr(), &raw const event) }
            } else {
                return Err(NativeSupplicantError::InvalidResult);
            };
            if status != 0 {
                return Err(NativeSupplicantError::FeedLinkFailed(status));
            }
            rx_work = true;
            rx_budget -= 1;
        }
        while rx_budget != 0 {
            let Some(external) = EXTERNAL_AUTH_QUEUE.take_oldest() else {
                break;
            };
            let event = external.event();
            // SAFETY: the fixed-size event is a complete deep copy and the
            // unique owner serializes this synchronous context call.
            let status =
                unsafe { hisi_wpa_feed_external_auth(self.context.as_ptr(), &raw const event) };
            if status != 0 {
                return Err(NativeSupplicantError::FeedExternalAuthFailed(status));
            }
            rx_work = true;
            rx_budget -= 1;
        }
        while rx_budget != 0 {
            let Some(frame) = MGMT_RX_QUEUE.take_oldest() else {
                break;
            };
            let meta = frame.meta();
            let bytes = frame.bytes();
            // SAFETY: the frame guard keeps the queue slot immutable for the
            // complete synchronous hostap event call.
            let status = unsafe {
                hisi_wpa_feed_mgmt(
                    self.context.as_ptr(),
                    meta.frequency_mhz,
                    meta.rssi_dbm,
                    bytes.as_ptr(),
                    bytes.len(),
                )
            };
            if status != 0 {
                return Err(NativeSupplicantError::FeedMgmtFailed(status));
            }
            rx_work = true;
            rx_budget -= 1;
        }
        let notified = EAPOL_PENDING.swap(false, Ordering::AcqRel);
        let fallback = !notified && self.eapol_fallback_polls_remaining != 0;
        if rx_budget != 0 && (notified || fallback) {
            if fallback {
                self.eapol_fallback_polls_remaining -= 1;
                DIAG_EAPOL_FALLBACK_POLLS.fetch_add(1, Ordering::Relaxed);
            }
            let received_before = DIAG_EAPOL_RECEIVED.load(Ordering::Relaxed);
            rx_work |= self.drain_eapol(&mut rx_budget)?;
            if fallback && DIAG_EAPOL_RECEIVED.load(Ordering::Relaxed) != received_before {
                DIAG_EAPOL_FALLBACK_HITS.fetch_add(1, Ordering::Relaxed);
            }
        }
        // SAFETY: the unique owner serializes all context calls.
        let mut result = unsafe {
            hisi_wpa_poll(
                self.context.as_ptr(),
                crate::uapi::monotonic_ms(),
                rx_budget.max(1),
            )
        };
        // SAFETY: the unique owner serializes this read-only snapshot with
        // every other access to the native context.
        DIAG_RECOVERY.store(
            unsafe { hisi_wpa_recovery_diagnostic_word(self.context.as_ptr()) },
            Ordering::Release,
        );
        if result.status != 0 {
            return Err(NativeSupplicantError::PollFailed(result.status));
        }
        if rx_work
            || SCAN_EVENT_QUEUE.has_pending()
            || LINK_EVENT_QUEUE.has_pending()
            || EXTERNAL_AUTH_QUEUE.has_pending()
            || MGMT_RX_QUEUE.has_pending()
            || EAPOL_PENDING.load(Ordering::Acquire)
        {
            result.work_pending = 1;
        }
        Ok(result)
    }

    fn drain_eapol(&mut self, budget: &mut u32) -> Result<bool, NativeSupplicantError> {
        let mut did_work = false;
        while *budget != 0 {
            let mut frame = [0_u8; MAX_EAPOL_RX_FRAME_LEN];
            let mut receive = RxEapol {
                buffer: frame.as_mut_ptr(),
                length: frame.len() as u32,
            };
            let status = crate::wal::ioctl(
                DRIVER_CONTEXT.ifname(),
                IOCTL_RECEIVE_EAPOL,
                (&mut receive as *mut RxEapol).cast(),
            );
            DIAG_EAPOL_RECEIVE_POLLS.fetch_add(1, Ordering::Relaxed);
            match classify_eapol_receive(status) {
                Ok(false) => break,
                Ok(true) => {}
                Err(status) => return Err(NativeSupplicantError::FeedEapolFailed(status)),
            }
            // Do not insert critical-section-backed diagnostics between an FFI
            // return and its decision point. On RV32IMFC, an interrupt-driven
            // task switch may occur when such a store restores MIE.
            DIAG_LAST_IOCTL_STATUS.store(status as u32, Ordering::Release);
            let len = receive.length as usize;
            if len <= ETHERNET_HEADER_LEN
                || len > frame.len()
                || frame[12..ETHERNET_HEADER_LEN] != EAPOL_ETHERTYPE
            {
                return Err(NativeSupplicantError::InvalidResult);
            }
            DIAG_EAPOL_RECEIVED.fetch_add(1, Ordering::Relaxed);
            let source = &frame[6..12];
            let payload = &frame[ETHERNET_HEADER_LEN..len];
            // SAFETY: source and payload remain live for the synchronous feed;
            // the unique owner prevents concurrent access to the C context.
            let fed = unsafe {
                hisi_wpa_feed_eapol(
                    self.context.as_ptr(),
                    source.as_ptr(),
                    payload.as_ptr(),
                    payload.len(),
                )
            };
            if fed != 0 {
                return Err(NativeSupplicantError::FeedEapolFailed(fed));
            }
            DIAG_EAPOL_FED.fetch_add(1, Ordering::Relaxed);
            did_work = true;
            *budget -= 1;
        }
        if *budget == 0 {
            EAPOL_PENDING.store(true, Ordering::Release);
        }
        Ok(did_work)
    }

    /// Drain one bounded event after [`Self::poll`].
    pub(crate) fn next_event(&mut self) -> Result<Option<Event>, NativeSupplicantError> {
        let mut event = core::mem::MaybeUninit::<Event>::uninit();
        // SAFETY: C writes the complete event when it returns one. The unique
        // owner prevents concurrent queue consumption.
        let result = unsafe { hisi_wpa_next_event(self.context.as_ptr(), event.as_mut_ptr()) };
        DIAG_EVENT_RING.store(self.event_ring_diagnostic_word(), Ordering::Release);
        match result {
            0 => Ok(None),
            1 => {
                // SAFETY: a return value of one is the ABI promise that the
                // output was initialized completely.
                let event = unsafe { event.assume_init() };
                if event.abi_version != ABI_VERSION || event.data_len as usize > event.data.len() {
                    Err(NativeSupplicantError::InvalidResult)
                } else {
                    DIAG_LAST_NATIVE_EVENT_KIND.store(u32::from(event.kind), Ordering::Release);
                    DIAG_LAST_NATIVE_EVENT_STATUS.store(event.status as u32, Ordering::Release);
                    Ok(Some(event))
                }
            }
            status => Err(NativeSupplicantError::PollFailed(status)),
        }
    }
}

fn classify_eapol_receive(status: c_int) -> Result<bool, c_int> {
    match status {
        0 => Ok(true),
        // The delivered WS63 `uapi_ioctl_receive_eapol` returns the positive
        // 16-bit sentinel 0xffff when its skb queue is empty. The vendor
        // LiteOS port treats this final empty receive as normal batch
        // termination; all other non-zero statuses remain real errors.
        0xffff => Ok(false),
        status => Err(status),
    }
}

const fn valid_context_layout(size: usize, alignment: usize) -> bool {
    size != 0 && alignment >= core::mem::align_of::<usize>() && alignment.is_power_of_two()
}

impl Drop for NativeSupplicant {
    fn drop(&mut self) {
        let _ = crate::wal::ioctl(
            DRIVER_CONTEXT.ifname(),
            IOCTL_DISABLE_EAPOL,
            DRIVER_CONTEXT.ifname().as_ptr().cast_mut().cast(),
        );
        EAPOL_PENDING.store(false, Ordering::Release);
        // SAFETY: this value is the unique owner and destroys the C context
        // before releasing its exact backing allocation.
        unsafe { hisi_wpa_destroy(self.context.as_ptr()) };
        crate::alloc::osal_kfree(self.storage.as_ptr());
    }
}

/// Copy one transient WS63 management RX event into the runner-owned queue.
#[cfg_attr(not(target_arch = "riscv32"), allow(dead_code))]
pub(crate) fn enqueue_mgmt_rx(frequency_mhz: u32, rssi_dbm: i32, frame: &[u8]) -> bool {
    if PORT_STATE.load(Ordering::Acquire) != PORT_READY {
        return false;
    }
    let queued = MGMT_RX_QUEUE.enqueue(frequency_mhz, rssi_dbm, frame);
    if queued {
        if DIAG_RX_AUTH.observe(frame, frequency_mhz) {
            DIAG_AUTH_RX_LAST.observe();
        }
        DIAG_MGMT_EVENTS.fetch_add(1, Ordering::Relaxed);
        let _ = RUNNER_WAKE.up();
    }
    queued
}

/// Deep-copy one WS63 external-auth trigger for the owning RadioRunner.
#[cfg_attr(not(target_arch = "riscv32"), allow(dead_code))]
pub(crate) fn enqueue_external_auth(
    action: u8,
    bssid: [u8; 6],
    ssid: &[u8],
    key_mgmt_suite: u32,
    status: u16,
    pmkid: Option<&[u8; 16]>,
) -> bool {
    if PORT_STATE.load(Ordering::Acquire) != PORT_READY
        || action > 1
        || ssid.len() > 32
        || (action == 0 && ssid.is_empty())
    {
        return false;
    }
    let mut event = ExternalAuthEvent {
        abi_version: ABI_VERSION,
        action,
        ssid_len: ssid.len() as u8,
        bssid,
        status,
        key_mgmt_suite,
        pmkid_present: u8::from(pmkid.is_some()),
        reserved: [0; 3],
        ssid: [0; 32],
        pmkid: [0; 16],
    };
    event.ssid[..ssid.len()].copy_from_slice(ssid);
    if let Some(pmkid) = pmkid {
        event.pmkid.copy_from_slice(pmkid);
    }
    let queued = EXTERNAL_AUTH_QUEUE.enqueue(event);
    if queued {
        if action == 0 {
            DIAG_EXTERNAL_AUTH_STARTED.observe();
        }
        let _ = RUNNER_WAKE.up();
    }
    queued
}

#[allow(clippy::too_many_arguments)]
#[cfg_attr(not(target_arch = "riscv32"), allow(dead_code))]
pub(crate) fn enqueue_scan_result(
    capabilities: u16,
    flags: u32,
    bssid: [u8; 6],
    frequency_mhz: i32,
    beacon_interval: u16,
    quality: i32,
    level_mbm: i32,
    age_ms: u32,
    ie_len: usize,
    beacon_ie_len: usize,
    ies: &[u8],
) -> bool {
    if PORT_STATE.load(Ordering::Acquire) != PORT_READY
        || !NATIVE_SCAN_ACTIVE.load(Ordering::Acquire)
    {
        return false;
    }
    let queued = SCAN_EVENT_QUEUE.enqueue_result(
        ScanMeta {
            kind: SCAN_EVENT_RESULT,
            capabilities,
            flags,
            bssid,
            frequency_mhz,
            beacon_interval,
            quality,
            level_mbm,
            age_ms,
            ie_len,
            beacon_ie_len,
            status: 0,
        },
        ies,
    );
    if queued {
        DIAG_SCAN_RESULTS.fetch_add(1, Ordering::Relaxed);
        let _ = RUNNER_WAKE.up();
    }
    queued
}

#[cfg_attr(not(target_arch = "riscv32"), allow(dead_code))]
pub(crate) fn enqueue_scan_done(status: i32) -> bool {
    if PORT_STATE.load(Ordering::Acquire) != PORT_READY
        || !NATIVE_SCAN_ACTIVE.swap(false, Ordering::AcqRel)
    {
        return false;
    }
    let queued = SCAN_EVENT_QUEUE.enqueue_done(status);
    if queued {
        DIAG_SCAN_DONE.fetch_add(1, Ordering::Relaxed);
        let _ = RUNNER_WAKE.up();
    }
    queued
}

#[cfg_attr(not(target_arch = "riscv32"), allow(dead_code))]
pub(crate) fn enqueue_associate_result(
    status: u16,
    frequency_mhz: u16,
    bssid: [u8; 6],
    request_ies: &[u8],
    response_ies: &[u8],
) -> bool {
    if PORT_STATE.load(Ordering::Acquire) != PORT_READY {
        return false;
    }
    let normalized_status = normalize_vendor_association_status(status);
    DIAG_ASSOC_RESULT_RAW_STATUS.store(u32::from(status), Ordering::Relaxed);
    DIAG_ASSOC_RESULT_STATUS.store(u32::from(normalized_status), Ordering::Relaxed);
    DIAG_ASSOC_RESULT_RESPONSE_IE_LEN.store(response_ies.len() as u32, Ordering::Relaxed);
    let sequence = DIAG_ASSOCIATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    DIAG_ASSOCIATION_ATTEMPTS[(sequence.wrapping_sub(1) as usize) % ASSOCIATION_ATTEMPT_CAPACITY]
        .record(sequence, status, normalized_status, response_ies);
    let deliver_as_disconnect = vendor_result_uses_disconnect(status);
    let (first, second) = if deliver_as_disconnect {
        (&[][..], &[][..])
    } else {
        (request_ies, response_ies)
    };
    let queued = LINK_EVENT_QUEUE.enqueue(
        LinkMeta {
            kind: if deliver_as_disconnect {
                LINK_EVENT_DISCONNECT
            } else {
                LINK_EVENT_ASSOCIATE
            },
            status_or_reason: if deliver_as_disconnect {
                status
            } else {
                normalized_status
            },
            frequency_mhz,
            bssid,
            first_len: first.len(),
            second_len: second.len(),
        },
        first,
        second,
    );
    if queued {
        DIAG_ASSOCIATE_EVENTS.fetch_add(1, Ordering::Relaxed);
        DIAG_ASSOCIATION_EVENT.observe();
        let _ = RUNNER_WAKE.up();
    }
    queued
}

#[cfg_attr(not(target_arch = "riscv32"), allow(dead_code))]
pub(crate) fn enqueue_disconnect(reason: u16, ies: &[u8]) -> bool {
    if PORT_STATE.load(Ordering::Acquire) != PORT_READY {
        return false;
    }
    let queued = LINK_EVENT_QUEUE.enqueue(
        LinkMeta {
            kind: LINK_EVENT_DISCONNECT,
            status_or_reason: reason,
            frequency_mhz: 0,
            bssid: [0; 6],
            first_len: ies.len(),
            second_len: 0,
        },
        ies,
        &[],
    );
    if queued {
        let _ = RUNNER_WAKE.up();
    }
    queued
}

unsafe extern "C" fn eapol_notify(_: *mut c_void, _: *mut c_void) {
    DIAG_EAPOL_EVENTS.fetch_add(1, Ordering::Relaxed);
    EAPOL_PENDING.store(true, Ordering::Release);
    let _ = RUNNER_WAKE.up();
}

/// Install the native OS/eloop hooks after `hisi-rtos` has installed its RF
/// runtime contract and after the WS63 ROM timebases are initialized.
///
/// The C boundary copies the hook table synchronously, so no Rust reference is
/// retained. Calling this repeatedly with the same hooks is idempotent.
pub fn prepare_upstream_supplicant_port(ifname: &[u8]) -> Result<(), UpstreamSupplicantPortError> {
    RUNNER_WAKE
        .try_init()
        .map_err(UpstreamSupplicantPortError::Runtime)?;

    match PORT_STATE.load(Ordering::Acquire) {
        PORT_READY => {
            return DRIVER_CONTEXT
                .matches(ifname)
                .then_some(())
                .ok_or(UpstreamSupplicantPortError::InterfaceConflict);
        }
        PORT_INSTALLING => return Err(UpstreamSupplicantPortError::Busy),
        PORT_POISONED => return Err(UpstreamSupplicantPortError::Poisoned),
        PORT_FREE => {}
        _ => return Err(UpstreamSupplicantPortError::Poisoned),
    }
    PORT_STATE
        .compare_exchange(
            PORT_FREE,
            PORT_INSTALLING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .map_err(|_| UpstreamSupplicantPortError::Busy)?;
    if !DRIVER_CONTEXT.initialize(ifname) {
        PORT_STATE.store(PORT_FREE, Ordering::Release);
        return Err(UpstreamSupplicantPortError::InvalidInterfaceName);
    }

    let os_hooks = OsHooks {
        abi_version: ABI_VERSION,
        reserved: 0,
        context: core::ptr::addr_of!(PORT_IDENTITY).cast_mut().cast(),
        allocate_zeroed: Some(allocate_zeroed),
        reallocate_zeroed: Some(reallocate_zeroed),
        deallocate: Some(deallocate),
        monotonic_us: Some(monotonic_us),
        wall_clock_us: None,
        sleep_ms: Some(sleep_ms),
        fill_entropy: Some(fill_entropy),
        wait_for_work: Some(wait_for_work),
        wake_runner: Some(wake_runner),
    };
    let driver_hooks = driver_hooks();
    // SAFETY: `os_hooks` matches ABI_VERSION and the C function copies the table
    // before returning. Every callback has C ABI and static backing state.
    let os_result = unsafe { hisi_wpa_os_install(&raw const os_hooks) };
    if os_result != 0 {
        PORT_STATE.store(PORT_FREE, Ordering::Release);
        return Err(UpstreamSupplicantPortError::Abi(os_result));
    }
    // SAFETY: the versioned table contains only C ABI callbacks with static
    // backing state and is copied by the C boundary before returning.
    let driver_result = unsafe { hisi_wpa_driver_install(&raw const driver_hooks) };
    if driver_result == 0 {
        PORT_STATE.store(PORT_READY, Ordering::Release);
        return Ok(());
    }

    // No driver/L2 user can exist before driver registration succeeds, so the
    // OS registration made above can be rolled back synchronously.
    let rollback = unsafe { hisi_wpa_os_uninstall(os_hooks.context) };
    if rollback == 0 {
        PORT_STATE.store(PORT_FREE, Ordering::Release);
        Err(UpstreamSupplicantPortError::Abi(driver_result))
    } else {
        PORT_STATE.store(PORT_POISONED, Ordering::Release);
        Err(UpstreamSupplicantPortError::Rollback {
            install: driver_result,
            rollback,
        })
    }
}

fn driver_hooks() -> DriverHooks {
    DriverHooks {
        abi_version: ABI_VERSION,
        reserved: 0,
        driver: core::ptr::addr_of!(DRIVER_CONTEXT).cast_mut().cast(),
        get_own_address: Some(get_own_address),
        get_driver_flags: Some(get_driver_flags),
        send_eapol: Some(send_eapol),
        send_mgmt: Some(send_mgmt),
        install_key: Some(install_key),
        remove_key: Some(remove_key),
        start_scan: Some(start_scan),
        associate: Some(associate),
        deauthenticate: Some(deauthenticate),
        send_external_auth_status: Some(send_external_auth_status),
    }
}

fn build_eapol_frame<'a>(
    destination: &[u8; 6],
    source: &[u8; 6],
    payload: &[u8],
    storage: &'a mut [u8; ETHERNET_HEADER_LEN + MAX_EAPOL_PAYLOAD_LEN],
) -> Option<&'a [u8]> {
    if payload.is_empty() || payload.len() > MAX_EAPOL_PAYLOAD_LEN {
        return None;
    }
    storage[..6].copy_from_slice(destination);
    storage[6..12].copy_from_slice(source);
    storage[12..ETHERNET_HEADER_LEN].copy_from_slice(&EAPOL_ETHERTYPE);
    storage[ETHERNET_HEADER_LEN..ETHERNET_HEADER_LEN + payload.len()].copy_from_slice(payload);
    Some(&storage[..ETHERNET_HEADER_LEN + payload.len()])
}

fn driver_context(driver: *mut c_void) -> Option<&'static DriverContext> {
    (driver == core::ptr::addr_of!(DRIVER_CONTEXT).cast_mut().cast()).then_some(&DRIVER_CONTEXT)
}

unsafe extern "C" fn get_own_address(driver: *mut c_void, address: *mut u8) -> c_int {
    let Some(driver) = driver_context(driver) else {
        return -1;
    };
    if address.is_null() {
        return -1;
    }
    if let Some(live) = crate::netif::hardware_address() {
        // SAFETY: the callback contract provides six writable bytes.
        unsafe { core::ptr::copy_nonoverlapping(live.as_ptr(), address, live.len()) };
        0
    } else {
        crate::wal::ioctl(driver.ifname(), IOCTL_GET_ADDRESS, address.cast())
    }
}

unsafe extern "C" fn get_driver_flags(driver: *mut c_void, flags: *mut u64) -> c_int {
    let Some(driver) = driver_context(driver) else {
        return -1;
    };
    let Some(flags) = (unsafe { flags.as_mut() }) else {
        return -1;
    };
    let status = crate::wal::ioctl(
        driver.ifname(),
        IOCTL_GET_DRIVER_FLAGS,
        (flags as *mut u64).cast(),
    );
    DIAG_DRIVER_FLAGS_LO.store(*flags as u32, Ordering::Release);
    DIAG_DRIVER_FLAGS_HI.store((*flags >> 32) as u32, Ordering::Release);
    DIAG_DRIVER_FLAGS_STATUS.store(status as u32, Ordering::Release);
    status
}

unsafe extern "C" fn send_eapol(
    driver: *mut c_void,
    destination: *const u8,
    payload: *const u8,
    payload_len: usize,
) -> c_int {
    let Some(driver) = driver_context(driver) else {
        return -1;
    };
    if destination.is_null()
        || payload.is_null()
        || payload_len == 0
        || payload_len > MAX_EAPOL_PAYLOAD_LEN
    {
        return -1;
    }
    let mut source = [0; 6];
    if unsafe {
        get_own_address(
            core::ptr::from_ref(driver).cast_mut().cast(),
            source.as_mut_ptr(),
        )
    } != 0
    {
        return -1;
    }
    // SAFETY: the callback contract supplies six destination bytes and
    // `payload_len` readable payload bytes for this synchronous call.
    let destination = unsafe { &*destination.cast::<[u8; 6]>() };
    let payload = unsafe { core::slice::from_raw_parts(payload, payload_len) };
    let mut storage = [0; ETHERNET_HEADER_LEN + MAX_EAPOL_PAYLOAD_LEN];
    let Some(frame) = build_eapol_frame(destination, &source, payload, &mut storage) else {
        return -1;
    };
    let mut request = TxEapol {
        buffer: frame.as_ptr().cast_mut(),
        length: frame.len() as u32,
    };
    let status = crate::wal::ioctl(
        driver.ifname(),
        IOCTL_SEND_EAPOL,
        (&mut request as *mut TxEapol).cast(),
    );
    if status == 0 {
        DIAG_EAPOL_SENDS.fetch_add(1, Ordering::Relaxed);
    }
    status
}

unsafe extern "C" fn send_mgmt(
    driver: *mut c_void,
    frequency_mhz: u32,
    frame: *const u8,
    frame_len: usize,
) -> c_int {
    let Some(driver) = driver_context(driver) else {
        return -1;
    };
    if frame.is_null() || frame_len == 0 || frame_len > u32::MAX as usize {
        return -1;
    }
    // SAFETY: null and zero length were rejected above. The callback contract
    // supplies `frame_len` readable bytes for this synchronous call.
    let frame_bytes = unsafe { core::slice::from_raw_parts(frame, frame_len) };
    let authentication_frame = DIAG_TX_AUTH.observe(frame_bytes, frequency_mhz);
    let mut request = MlmeData {
        frequency_mhz,
        data_len: frame_len as u32,
        data: frame.cast_mut(),
        send_action_cookie: driver.send_action_cookie.get(),
    };
    let status = crate::wal::ioctl(
        driver.ifname(),
        IOCTL_SEND_MLME,
        (&mut request as *mut MlmeData).cast(),
    );
    DIAG_LAST_IOCTL_STATUS.store(status as u32, Ordering::Release);
    if authentication_frame && status == 0 {
        DIAG_AUTH_TX_LAST.observe();
    }
    if status != 0 {
        // Only the public 802.11/authentication header is emitted. The SAE
        // scalar/element payload starts after this prefix and must never enter
        // logs.
        let header_len = frame_len.min(30);
        // SAFETY: null and zero length were rejected above; header_len is
        // bounded by the caller-provided frame length.
        let header = unsafe { core::slice::from_raw_parts(frame, header_len) };
        crate::log_emit(b"RFDBG_WPA_SEND_MGMT_ERR status=");
        emit_diagnostic_hex(status as u32);
        crate::log_emit(b" len=");
        emit_diagnostic_hex(frame_len as u32);
        crate::log_emit(b" header=");
        emit_diagnostic_bytes(header);
        crate::log_emit(b"\r\n");
    }
    status
}

pub(crate) fn emit_diagnostic_hex(value: u32) {
    let mut output = *b"0x00000000";
    for index in 0..8 {
        let nibble = ((value >> ((7 - index) * 4)) & 0x0f) as u8;
        output[index + 2] = if nibble < 10 {
            b'0' + nibble
        } else {
            b'a' + nibble - 10
        };
    }
    crate::log_emit(&output);
}

pub(crate) fn emit_diagnostic_bytes(bytes: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = [0_u8; 60];
    for (index, byte) in bytes.iter().copied().enumerate() {
        encoded[index * 2] = HEX[(byte >> 4) as usize];
        encoded[index * 2 + 1] = HEX[(byte & 0x0f) as usize];
    }
    crate::log_emit(&encoded[..bytes.len() * 2]);
}

unsafe extern "C" fn send_external_auth_status(
    driver: *mut c_void,
    status: *const ExternalAuthStatus,
) -> c_int {
    let Some(driver) = driver_context(driver) else {
        return -1;
    };
    let Some(status) = (unsafe { status.as_ref() }) else {
        return -1;
    };
    if status.abi_version != ABI_VERSION || status.pmkid_present > 1 {
        return -1;
    }
    let mut request = VendorExternalAuthStatus {
        action: 0,
        bssid: status.bssid,
        reserved0: 0,
        ssid: core::ptr::null_mut(),
        ssid_len: 0,
        key_mgmt_suite: 0,
        status: status.status,
        reserved1: [0; 2],
        pmkid: if status.pmkid_present == 0 {
            core::ptr::null_mut()
        } else {
            status.pmkid.as_ptr().cast_mut()
        },
    };
    let result = crate::wal::ioctl(
        driver.ifname(),
        IOCTL_SEND_EXTERNAL_AUTH_STATUS,
        (&mut request as *mut VendorExternalAuthStatus).cast(),
    );
    DIAG_LAST_IOCTL_STATUS.store(result as u32, Ordering::Release);
    if result == 0 {
        DIAG_EXTERNAL_AUTH_STATUS_SENT.observe();
    }
    result
}

unsafe extern "C" fn install_key(
    driver: *mut c_void,
    key: *const Key,
    material: *const u8,
    material_len: usize,
) -> c_int {
    let Some(driver) = driver_context(driver) else {
        return -1;
    };
    let Some(key) = (unsafe { key.as_ref() }) else {
        return -1;
    };
    if material.is_null() || !valid_key_material(key.cipher, material_len) {
        return -1;
    }
    let Some(mut request) = key_request(key, material.cast_mut(), material_len) else {
        return -1;
    };
    DIAG_KEY_INSTALLS.fetch_add(1, Ordering::Relaxed);
    let install = crate::wal::ioctl(
        driver.ifname(),
        IOCTL_NEW_KEY,
        (&mut request as *mut KeyExtension).cast(),
    );
    if install != 0 || key.flags & key_flag::TX == 0 {
        return install;
    }
    let set_default = crate::wal::ioctl(
        driver.ifname(),
        IOCTL_SET_KEY,
        (&mut request as *mut KeyExtension).cast(),
    );
    if set_default != 0 {
        let _ = crate::wal::ioctl(
            driver.ifname(),
            IOCTL_DEL_KEY,
            (&mut request as *mut KeyExtension).cast(),
        );
    }
    set_default
}

unsafe extern "C" fn remove_key(driver: *mut c_void, key: *const Key) -> c_int {
    let Some(driver) = driver_context(driver) else {
        return -1;
    };
    let Some(key) = (unsafe { key.as_ref() }) else {
        return -1;
    };
    let Some(mut request) = key_request(key, core::ptr::null_mut(), 0) else {
        return -1;
    };
    crate::wal::ioctl(
        driver.ifname(),
        IOCTL_DEL_KEY,
        (&mut request as *mut KeyExtension).cast(),
    )
}

unsafe extern "C" fn start_scan(driver: *mut c_void, request: *const ScanRequest) -> c_int {
    let Some(driver) = driver_context(driver) else {
        return -1;
    };
    let Some(request) = (unsafe { request.as_ref() }) else {
        return -1;
    };
    let ssid_len = request.ssid_len as usize;
    let frequency_count = request.num_frequencies as usize;
    if request.abi_version != ABI_VERSION
        || ssid_len > request.ssid.len()
        || frequency_count > MAX_SCAN_FREQUENCIES
        || request.bssid_present > 1
        || request.extra_ies_len > MAX_SCAN_IE_LEN
        || (request.extra_ies_len != 0 && request.extra_ies.is_null())
    {
        return -1;
    }
    let mut ssid = VendorScanSsid {
        ssid: request.ssid,
        ssid_len: ssid_len as u32,
    };
    let mut scan = VendorScanRequest {
        // The WS63 ioctl models wildcard scan as one descriptor whose
        // ssid_len is zero. Passing zero descriptors is rejected, even though
        // upstream hostap uses an empty SSID to express the same wildcard.
        ssids: &mut ssid,
        frequencies: if frequency_count == 0 {
            core::ptr::null_mut()
        } else {
            request.frequencies.as_ptr().cast_mut()
        },
        extra_ies: request.extra_ies.cast_mut(),
        bssid: if request.bssid_present == 0 {
            core::ptr::null_mut()
        } else {
            request.bssid.as_ptr().cast_mut()
        },
        num_ssids: 1,
        num_frequencies: request.num_frequencies,
        prefix_ssid_scan: 0,
        fast_connect: 0,
        extra_ies_len: request.extra_ies_len as u32,
        acs_scan: 0,
    };
    if NATIVE_SCAN_ACTIVE
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return -1;
    }
    DIAG_SCAN_STARTS.fetch_add(1, Ordering::Relaxed);
    SCAN_EVENT_QUEUE.begin_transaction();
    let status = crate::wal::ioctl(
        driver.ifname(),
        IOCTL_SCAN,
        (&mut scan as *mut VendorScanRequest).cast(),
    );
    if status != 0 {
        NATIVE_SCAN_ACTIVE.store(false, Ordering::Release);
    }
    DIAG_LAST_IOCTL_STATUS.store(status as u32, Ordering::Release);
    status
}

const fn vendor_sae_pwe(value: u8) -> Option<i32> {
    match value {
        value if value == SaePwe::HuntAndPeck as u8 => Some(1),
        value if value == SaePwe::HashToElement as u8 => Some(2),
        value if value == SaePwe::Both as u8 => Some(3),
        _ => None,
    }
}

unsafe extern "C" fn associate(driver: *mut c_void, request: *const AssociateRequest) -> c_int {
    let Some(driver) = driver_context(driver) else {
        return -1;
    };
    let Some(request) = (unsafe { request.as_ref() }) else {
        return -1;
    };
    let ssid_len = request.ssid_len as usize;
    if request.abi_version != ABI_VERSION
        || ssid_len == 0
        || ssid_len > request.ssid.len()
        || request.bssid_present > 1
        || request.pmf > Pmf::Required as u8
        || request.sae_pwe > SaePwe::Both as u8
        || !matches!(request.auth_type, 0 | 3)
        || request.frequency_mhz == 0
        || request.pairwise_suite == 0
        || request.group_suite == 0
        || request.key_mgmt_suite == 0
        || request.association_ies_len > MAX_ASSOCIATION_IE_LEN
        || (request.association_ies_len != 0 && request.association_ies.is_null())
    {
        return -1;
    }
    let Some(sae_pwe) = vendor_sae_pwe(request.sae_pwe) else {
        return -1;
    };
    let mut crypto = VendorCryptoSettings {
        wpa_versions: request.wpa_versions,
        cipher_group: request.group_suite,
        num_pairwise: 1,
        pairwise: [request.pairwise_suite, 0, 0, 0, 0],
        num_akm: 1,
        akm: [request.key_mgmt_suite, 0],
        // hostap uses 0/1/2 for hunt-and-peck/H2E/both. The WS63 UAPI adds
        // UNSPECIFIED at zero, so the firmware contract is 1/2/3.
        sae_pwe,
    };
    let mut association = VendorAssociateRequest {
        bssid: if request.bssid_present == 0 {
            core::ptr::null_mut()
        } else {
            request.bssid.as_ptr().cast_mut()
        },
        ssid: request.ssid.as_ptr().cast_mut(),
        ies: request.association_ies.cast_mut(),
        key: core::ptr::null_mut(),
        auth_type: request.auth_type,
        privacy: request.privacy,
        key_len: 0,
        key_index: 0,
        pmf: request.pmf,
        auto_connect: 0,
        reserved: [0; 2],
        frequency_mhz: request.frequency_mhz,
        ssid_len: ssid_len as u32,
        ies_len: request.association_ies_len as u32,
        crypto: &mut crypto,
    };
    DIAG_ASSOCIATE_CALLS.fetch_add(1, Ordering::Relaxed);
    let first_status = crate::wal::ioctl(
        driver.ifname(),
        IOCTL_ASSOCIATE,
        (&mut association as *mut VendorAssociateRequest).cast(),
    );
    let mut status = first_status;
    let mut disconnect_status = 0;
    if status != 0 {
        DIAG_ASSOCIATE_RETRIES.fetch_add(1, Ordering::Relaxed);
        // Match the vendor driver_soc connection contract: firmware can retain
        // stale station state after scan/previous attempts, so one failed
        // association is recovered by an explicit disconnect and one retry.
        // This is bounded and preserves the final ioctl status for diagnosis.
        let mut reason = WLAN_REASON_PREV_AUTH_NOT_VALID;
        disconnect_status = crate::wal::ioctl(
            driver.ifname(),
            IOCTL_DISCONNECT,
            (&mut reason as *mut u16).cast(),
        );
        if disconnect_status == 0 {
            status = crate::wal::ioctl(
                driver.ifname(),
                IOCTL_ASSOCIATE,
                (&mut association as *mut VendorAssociateRequest).cast(),
            );
        }
    }
    if status != 0 {
        crate::log_emit(b"RFDBG_WPA_ASSOC_ERR first=");
        emit_diagnostic_hex(first_status as u32);
        crate::log_emit(b" disconnect=");
        emit_diagnostic_hex(disconnect_status as u32);
        crate::log_emit(b" retry=");
        emit_diagnostic_hex(status as u32);
        crate::log_emit(b" auth=");
        emit_diagnostic_hex(u32::from(association.auth_type));
        crate::log_emit(b" pmf=");
        emit_diagnostic_hex(u32::from(association.pmf));
        crate::log_emit(b" pwe=");
        emit_diagnostic_hex(crypto.sae_pwe as u32);
        crate::log_emit(b" wpa=");
        emit_diagnostic_hex(crypto.wpa_versions);
        crate::log_emit(b" pairwise=");
        emit_diagnostic_hex(crypto.pairwise[0]);
        crate::log_emit(b" group=");
        emit_diagnostic_hex(crypto.cipher_group);
        crate::log_emit(b" akm=");
        emit_diagnostic_hex(crypto.akm[0]);
        crate::log_emit(b" freq=");
        emit_diagnostic_hex(association.frequency_mhz);
        crate::log_emit(b" ie_len=");
        emit_diagnostic_hex(association.ies_len);
        crate::log_emit(b"\r\n");
    }
    DIAG_LAST_IOCTL_STATUS.store(status as u32, Ordering::Release);
    DIAG_ASSOCIATE_AUTH.store(u32::from(association.auth_type), Ordering::Release);
    DIAG_ASSOCIATE_PMF.store(u32::from(association.pmf), Ordering::Release);
    DIAG_ASSOCIATE_PWE.store(crypto.sae_pwe as u32, Ordering::Release);
    DIAG_ASSOCIATE_AKM.store(crypto.akm[0], Ordering::Release);
    DIAG_ASSOCIATE_STATUS.store(status as u32, Ordering::Release);
    status
}

pub(crate) fn diagnostic_snapshot() -> [u32; 11] {
    [
        DIAG_DRIVER_FLAGS_STATUS.load(Ordering::Acquire),
        DIAG_DRIVER_FLAGS_LO.load(Ordering::Acquire),
        DIAG_DRIVER_FLAGS_HI.load(Ordering::Acquire),
        DIAG_ASSOCIATE_STATUS.load(Ordering::Acquire),
        DIAG_ASSOCIATE_AUTH.load(Ordering::Acquire),
        DIAG_ASSOCIATE_PMF.load(Ordering::Acquire),
        DIAG_ASSOCIATE_PWE.load(Ordering::Acquire),
        DIAG_ASSOCIATE_AKM.load(Ordering::Acquire),
        DIAG_EXTERNAL_AUTH_CALLBACKS.load(Ordering::Acquire),
        DIAG_EXTERNAL_AUTH_REJECTS.load(Ordering::Acquire),
        DIAG_EXTERNAL_AUTH_LENGTH.load(Ordering::Acquire),
    ]
}

pub(crate) fn authentication_diagnostic_snapshot() -> [u32; 12] {
    let tx = DIAG_TX_AUTH.snapshot();
    let rx = DIAG_RX_AUTH.snapshot();
    [
        tx[0], tx[1], tx[2], tx[3], tx[4], tx[5], rx[0], rx[1], rx[2], rx[3], rx[4], rx[5],
    ]
}

pub(crate) fn authentication_progress_snapshot() -> [u32; 10] {
    let started = DIAG_EXTERNAL_AUTH_STARTED.snapshot();
    let tx = DIAG_AUTH_TX_LAST.snapshot();
    let rx = DIAG_AUTH_RX_LAST.snapshot();
    let status = DIAG_EXTERNAL_AUTH_STATUS_SENT.snapshot();
    let association = DIAG_ASSOCIATION_EVENT.snapshot();
    [
        started[0],
        started[1],
        tx[0],
        tx[1],
        rx[0],
        rx[1],
        status[0],
        status[1],
        association[0],
        association[1],
    ]
}

pub(crate) fn eapol_diagnostic_snapshot() -> [u32; 8] {
    [
        DIAG_EAPOL_EVENTS.load(Ordering::Acquire),
        DIAG_EAPOL_RECEIVE_POLLS.load(Ordering::Acquire),
        DIAG_EAPOL_RECEIVED.load(Ordering::Acquire),
        DIAG_EAPOL_FED.load(Ordering::Acquire),
        DIAG_EAPOL_SENDS.load(Ordering::Acquire),
        DIAG_KEY_INSTALLS.load(Ordering::Acquire),
        DIAG_EAPOL_FALLBACK_POLLS.load(Ordering::Acquire),
        DIAG_EAPOL_FALLBACK_HITS.load(Ordering::Acquire),
    ]
}

pub(crate) fn association_attempt_diagnostics(
    output: &mut [AssociationAttemptDiagnostic],
) -> usize {
    let mut retained = [AssociationAttemptDiagnostic::default(); ASSOCIATION_ATTEMPT_CAPACITY];
    let mut count = 0;
    for attempt in &DIAG_ASSOCIATION_ATTEMPTS {
        let snapshot = attempt.snapshot();
        if snapshot.sequence != 0 {
            retained[count] = snapshot;
            count += 1;
        }
    }
    retained[..count].sort_unstable_by_key(|attempt| attempt.sequence);
    let copied = usize::min(count, output.len());
    output[..copied].copy_from_slice(&retained[..copied]);
    copied
}

pub(crate) fn event_diagnostic_snapshot() -> [u32; 6] {
    [
        DIAG_EVENT_RING.load(Ordering::Acquire),
        DIAG_LAST_NATIVE_EVENT_KIND.load(Ordering::Acquire),
        DIAG_LAST_NATIVE_EVENT_STATUS.load(Ordering::Acquire),
        DIAG_ASSOC_RESULT_RAW_STATUS.load(Ordering::Acquire),
        DIAG_ASSOC_RESULT_STATUS.load(Ordering::Acquire),
        DIAG_ASSOC_RESULT_RESPONSE_IE_LEN.load(Ordering::Acquire),
    ]
}

pub(crate) fn recovery_diagnostic_word() -> u32 {
    DIAG_RECOVERY.load(Ordering::Acquire)
}

pub(crate) fn temporary_reject_recovery_diagnostic_snapshot() -> [u32; 3] {
    [
        DIAG_TEMP_REJECT_CLEARS.load(Ordering::Acquire),
        DIAG_TEMP_REJECT_CLEAR_FAILURES.load(Ordering::Acquire),
        DIAG_TEMP_REJECT_CLEAR_STATUS.load(Ordering::Acquire),
    ]
}

#[cfg(target_arch = "riscv32")]
pub(crate) fn observe_external_auth_callback(length: u32) {
    DIAG_EXTERNAL_AUTH_CALLBACKS.fetch_add(1, Ordering::Relaxed);
    DIAG_EXTERNAL_AUTH_LENGTH.store(length, Ordering::Release);
}

#[cfg(target_arch = "riscv32")]
pub(crate) fn reject_external_auth_callback() {
    DIAG_EXTERNAL_AUTH_REJECTS.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn diagnostic_word() -> u32 {
    let starts = u32::from(DIAG_SCAN_STARTS.load(Ordering::Relaxed) != 0);
    let results = u32::from(DIAG_SCAN_RESULTS.load(Ordering::Relaxed) != 0);
    let done = u32::from(DIAG_SCAN_DONE.load(Ordering::Relaxed) != 0);
    let associate = u32::from(DIAG_ASSOCIATE_CALLS.load(Ordering::Relaxed) != 0);
    let associate_event = u32::from(DIAG_ASSOCIATE_EVENTS.load(Ordering::Relaxed) != 0);
    let mgmt = u32::from(DIAG_MGMT_EVENTS.load(Ordering::Relaxed) != 0);
    let eapol = u32::from(DIAG_EAPOL_EVENTS.load(Ordering::Relaxed) != 0);
    let key = u32::from(DIAG_KEY_INSTALLS.load(Ordering::Relaxed) != 0);
    // SAFETY: the native supplicant owner serializes eloop access. This is a
    // read-only snapshot used only after a bounded operation times out.
    let eloop = unsafe { hisi_wpa_eloop_diagnostic_flags() } & 0x0f;
    // SAFETY: the driver adapter exposes a read-only diagnostic nibble.
    let mut driver = unsafe { hisi_wpa_driver_diagnostic_word() } & 0x0f;
    if driver == 1 && DIAG_ASSOCIATE_RETRIES.load(Ordering::Relaxed) != 0 {
        // Reserved stage: the C adapter accepted the request and the Rust WAL
        // hook succeeded only after its bounded recovery retry.
        driver = 0x0f;
    }
    starts
        | (results << 1)
        | (done << 2)
        | (associate << 3)
        | (associate_event << 4)
        | (mgmt << 5)
        | (eapol << 6)
        | (key << 7)
        | (eloop << 8)
        | (driver << 12)
}

unsafe extern "C" fn deauthenticate(driver: *mut c_void, reason: u16) -> c_int {
    let Some(driver) = driver_context(driver) else {
        return -1;
    };
    let mut reason = reason;
    crate::wal::ioctl(
        driver.ifname(),
        IOCTL_DISCONNECT,
        (&mut reason as *mut u16).cast(),
    )
}

fn key_request(key: &Key, material: *mut u8, material_len: usize) -> Option<KeyExtension> {
    const ALLOWED_FLAGS: u32 =
        key_flag::DEFAULT | key_flag::RX | key_flag::TX | key_flag::GROUP | key_flag::PAIRWISE;
    if key.abi_version != ABI_VERSION
        || key.sequence_len as usize > key.sequence.len()
        || key.flags & !ALLOWED_FLAGS != 0
        || key.flags & (key_flag::MODIFY | key_flag::PMK) != 0
        || key.flags & (key_flag::RX | key_flag::TX) == 0
    {
        return None;
    }
    let pairwise = key.flags & key_flag::PAIRWISE != 0;
    let group = key.flags & key_flag::GROUP != 0;
    if pairwise == group {
        return None;
    }
    let broadcast = key.peer == [0xff; 6];
    if key.peer_present > 1
        || (pairwise && (key.peer_present != 1 || broadcast))
        || (group && key.peer_present == 1 && !broadcast)
    {
        return None;
    }
    let address = if pairwise {
        key.peer.as_ptr().cast_mut()
    } else {
        core::ptr::null_mut()
    };
    let management = matches!(
        key.cipher,
        cipher::BIP_CMAC_128 | cipher::BIP_GMAC_128 | cipher::BIP_GMAC_256 | cipher::BIP_CMAC_256
    );
    Some(KeyExtension {
        key_type: if pairwise {
            KEY_TYPE_PAIRWISE
        } else {
            KEY_TYPE_GROUP
        },
        key_index: key.key_index as u32,
        key_len: material_len as u32,
        sequence_len: key.sequence_len as u32,
        cipher: cipher_suite(key.cipher, material_len)?,
        address,
        material,
        sequence: if key.sequence_len != 0 {
            key.sequence.as_ptr().cast_mut()
        } else {
            core::ptr::null_mut()
        },
        default_data: (!management) as u8,
        default_management: management as u8,
        default_types: if pairwise {
            KEY_DEFAULT_UNICAST
        } else if key.peer_present == 1 {
            KEY_DEFAULT_MULTICAST
        } else {
            KEY_DEFAULT_INVALID
        },
        reserved: 0,
    })
}

fn valid_key_material(cipher: u8, len: usize) -> bool {
    match cipher {
        cipher::WEP => matches!(len, 5 | 13),
        cipher::TKIP
        | cipher::GCMP_256
        | cipher::CCMP_256
        | cipher::BIP_GMAC_256
        | cipher::BIP_CMAC_256 => len == 32,
        cipher::CCMP | cipher::BIP_CMAC_128 | cipher::GCMP | cipher::BIP_GMAC_128 => len == 16,
        _ => false,
    }
}

fn cipher_suite(cipher: u8, material_len: usize) -> Option<u32> {
    let selector = match cipher {
        cipher::NONE => 0,
        cipher::WEP if material_len == 5 => 1,
        cipher::WEP if material_len == 13 => 5,
        cipher::TKIP => 2,
        cipher::CCMP => 4,
        cipher::BIP_CMAC_128 => 6,
        cipher::GCMP => 8,
        cipher::GCMP_256 => 9,
        cipher::CCMP_256 => 10,
        cipher::BIP_GMAC_128 => 11,
        cipher::BIP_GMAC_256 => 12,
        cipher::BIP_CMAC_256 => 13,
        _ => return None,
    };
    Some(if selector == 0 {
        0
    } else {
        0x000f_ac00 | selector
    })
}

unsafe extern "C" fn allocate_zeroed(_: *mut c_void, size: usize, alignment: usize) -> *mut c_void {
    crate::alloc::allocate_zeroed(size, alignment)
}

unsafe extern "C" fn reallocate_zeroed(
    _: *mut c_void,
    pointer: *mut c_void,
    size: usize,
    alignment: usize,
) -> *mut c_void {
    // SAFETY: the C port passes only null or pointers obtained through the
    // matching allocation callback. CHeap validates ownership defensively.
    unsafe { crate::alloc::reallocate_zeroed(pointer, size, alignment) }
}

unsafe extern "C" fn deallocate(_: *mut c_void, pointer: *mut c_void) {
    crate::alloc::osal_kfree(pointer);
}

unsafe extern "C" fn monotonic_us(_: *mut c_void, value: *mut u64) -> c_int {
    let Some(value) = (unsafe { value.as_mut() }) else {
        return -1;
    };
    *value = crate::uapi::monotonic_us();
    0
}

unsafe extern "C" fn sleep_ms(_: *mut c_void, milliseconds: u32) -> c_int {
    let result = if let Some(milliseconds) = NonZeroU32::new(milliseconds) {
        hisi_rf_rtos_driver::sleep_ms(milliseconds)
    } else {
        hisi_rf_rtos_driver::yield_now()
    };
    result.map(|()| 0).unwrap_or(-1)
}

unsafe extern "C" fn fill_entropy(_: *mut c_void, output: *mut u8, output_len: usize) -> c_int {
    if output_len == 0 {
        return 0;
    }
    if output.is_null() {
        return -1;
    }
    #[cfg(target_arch = "riscv32")]
    {
        // SAFETY: null was rejected and the C ABI promises `output_len`
        // writable bytes for the duration of this call.
        let output = unsafe { core::slice::from_raw_parts_mut(output, output_len) };
        crate::crypto::fill_hardware_entropy(output)
            .map(|()| 0)
            .unwrap_or(-1)
    }
    #[cfg(not(target_arch = "riscv32"))]
    {
        let _ = output;
        -1
    }
}

unsafe extern "C" fn wait_for_work(_: *mut c_void, timeout_ms: u32) -> c_int {
    match RUNNER_WAKE.down_timeout(WaitTimeout::from_millis(timeout_ms)) {
        Ok(WaitOutcome::Acquired) | Ok(WaitOutcome::TimedOut) => 0,
        Err(_) => -1,
    }
}

unsafe extern "C" fn wake_runner(_: *mut c_void) {
    let _ = RUNNER_WAKE.up();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_registration_before_runtime_installation() {
        assert_eq!(
            prepare_upstream_supplicant_port(b"wlan0"),
            Err(UpstreamSupplicantPortError::Runtime(
                hisi_rf_rtos_driver::Error::NotInstalled
            ))
        );
    }

    #[test]
    fn rejects_invalid_opaque_context_layouts() {
        let natural = core::mem::align_of::<usize>();
        assert!(valid_context_layout(1, natural));
        assert!(!valid_context_layout(0, natural));
        assert!(!valid_context_layout(1, natural.saturating_sub(1)));
        assert!(!valid_context_layout(1, natural + 1));
    }

    #[test]
    fn builds_bounded_ethernet_eapol_frame() {
        let destination = [1, 2, 3, 4, 5, 6];
        let source = [6, 5, 4, 3, 2, 1];
        let payload = [2, 3, 0, 5];
        let mut storage = [0; ETHERNET_HEADER_LEN + MAX_EAPOL_PAYLOAD_LEN];
        let frame = build_eapol_frame(&destination, &source, &payload, &mut storage).unwrap();
        assert_eq!(&frame[..6], &destination);
        assert_eq!(&frame[6..12], &source);
        assert_eq!(&frame[12..14], &EAPOL_ETHERTYPE);
        assert_eq!(&frame[14..], &payload);
    }

    #[test]
    fn rejects_empty_or_oversized_eapol_payload() {
        let mut storage = [0; ETHERNET_HEADER_LEN + MAX_EAPOL_PAYLOAD_LEN];
        assert!(build_eapol_frame(&[0; 6], &[0; 6], &[], &mut storage).is_none());
        assert!(
            build_eapol_frame(
                &[0; 6],
                &[0; 6],
                &[0; MAX_EAPOL_PAYLOAD_LEN + 1],
                &mut storage,
            )
            .is_none()
        );
    }

    #[test]
    fn ws63_empty_eapol_sentinel_ends_the_current_drain_batch() {
        assert_eq!(classify_eapol_receive(0), Ok(true));
        assert_eq!(classify_eapol_receive(0xffff), Ok(false));
        assert_eq!(classify_eapol_receive(-1), Err(-1));
        assert_eq!(classify_eapol_receive(-22), Err(-22));
        assert_eq!(classify_eapol_receive(1), Err(1));
    }

    #[test]
    fn management_queue_is_bounded_and_fifo() {
        let queue = MgmtRxQueue::new();
        for index in 0..MGMT_RX_QUEUE_DEPTH {
            assert!(queue.enqueue(2412 + index as u32, -30, &[index as u8; 4]));
        }
        assert!(!queue.enqueue(5200, -40, &[0xaa; 4]));
        assert_eq!(queue.dropped.load(Ordering::Relaxed), 1);
        for index in 0..MGMT_RX_QUEUE_DEPTH {
            let frame = queue.take_oldest().unwrap();
            assert_eq!(frame.meta().frequency_mhz, 2412 + index as u32);
            assert_eq!(frame.bytes(), &[index as u8; 4]);
        }
        assert!(!queue.has_pending());
    }

    #[test]
    fn management_queue_rejects_invalid_lengths() {
        let queue = MgmtRxQueue::new();
        assert!(!queue.enqueue(2412, -30, &[]));
        assert!(!queue.enqueue(2412, -30, &[0; MAX_MGMT_FRAME_LEN + 1]));
        assert_eq!(queue.dropped.load(Ordering::Relaxed), 2);
        assert!(queue.take_oldest().is_none());
    }

    #[test]
    fn parses_only_public_authentication_header_fields() {
        let mut frame = [0xa5; 64];
        frame[0] = 0xb0;
        frame[1] = 0;
        frame[24..26].copy_from_slice(&3_u16.to_le_bytes());
        frame[26..28].copy_from_slice(&2_u16.to_le_bytes());
        frame[28..30].copy_from_slice(&76_u16.to_le_bytes());
        assert_eq!(
            authentication_header(&frame),
            Some(AuthenticationHeader {
                algorithm: 3,
                transaction: 2,
                status: 76,
            })
        );

        frame[0] = 0xd0;
        assert_eq!(authentication_header(&frame), None);
        assert_eq!(authentication_header(&frame[..29]), None);
    }

    #[test]
    fn normalizes_only_vendor_offset_association_status_codes() {
        assert_eq!(normalize_vendor_association_status(0), 0);
        assert_eq!(normalize_vendor_association_status(30), 30);
        assert_eq!(normalize_vendor_association_status(8_030), 30);
        assert_eq!(normalize_vendor_association_status(8_053), 53);
        assert_eq!(normalize_vendor_association_status(5_203), 5_203);
        assert_eq!(normalize_vendor_association_status(8_256), 8_256);
    }

    #[test]
    fn parses_temporary_reject_interval_and_matches_vendor_event_oracle() {
        let response = [1, 1, 0xaa, 56, 5, 3, 0x34, 0x12, 0, 0];
        assert_eq!(association_comeback_interval(&response), Some(0x1234));
        assert_eq!(association_comeback_interval(&[]), None);
        assert!(!vendor_result_uses_association_reject(8_030));
        assert!(vendor_result_requires_stale_state_clear(8_030));
        assert!(!vendor_result_requires_stale_state_clear(30));
        assert!(!vendor_result_requires_stale_state_clear(8_053));
        assert!(vendor_result_uses_disconnect(8_030));
        assert!(vendor_result_uses_disconnect(30));
        assert!(!vendor_result_uses_disconnect(8_053));
        assert!(vendor_result_uses_association_reject(8_053));
        assert!(vendor_result_uses_association_reject(7_016));
        assert!(!vendor_result_uses_disconnect(0));
    }

    #[test]
    fn scan_queue_preserves_event_order_and_deep_copies() {
        let queue = ScanEventQueue::new();
        let mut ies = [1, 2, 3, 4];
        assert!(queue.enqueue_result(
            ScanMeta {
                kind: SCAN_EVENT_RESULT,
                capabilities: 0x10,
                flags: 7,
                bssid: [1, 2, 3, 4, 5, 6],
                frequency_mhz: 2412,
                beacon_interval: 100,
                quality: 20,
                level_mbm: -4200,
                age_ms: 3,
                ie_len: 3,
                beacon_ie_len: 1,
                status: 0,
            },
            &ies,
        ));
        ies.fill(0xff);
        assert!(queue.enqueue_done(2));
        let result = queue.take_oldest().unwrap();
        assert_eq!(result.meta().frequency_mhz, 2412);
        assert_eq!(result.ies(), &[1, 2, 3, 4]);
        drop(result);
        let done = queue.take_oldest().unwrap();
        assert_eq!(done.meta().kind, SCAN_EVENT_DONE);
        assert_eq!(done.meta().status, 2);
    }

    #[test]
    fn scan_queue_reserves_done_slot_when_results_are_truncated() {
        let queue = ScanEventQueue::new();
        queue.begin_transaction();
        for index in 0..crate::wifi::MAX_SCAN_RESULTS + 1 {
            assert!(queue.enqueue_result(
                ScanMeta {
                    kind: SCAN_EVENT_RESULT,
                    capabilities: 0,
                    flags: 0,
                    bssid: [index as u8; 6],
                    frequency_mhz: 2412,
                    beacon_interval: 100,
                    quality: 0,
                    level_mbm: -4000,
                    age_ms: 0,
                    ie_len: 0,
                    beacon_ie_len: 0,
                    status: 0,
                },
                &[],
            ));
        }
        assert_eq!(queue.truncated_results.load(Ordering::Relaxed), 1);
        assert_eq!(queue.dropped.load(Ordering::Relaxed), 0);
        assert!(queue.enqueue_done(0));

        let mut results = 0;
        while let Some(event) = queue.take_oldest() {
            if event.meta().kind == SCAN_EVENT_RESULT {
                results += 1;
            } else {
                assert_eq!(event.meta().kind, SCAN_EVENT_DONE);
            }
        }
        assert_eq!(results, crate::wifi::MAX_SCAN_RESULTS);
    }

    #[test]
    fn link_queue_is_bounded_and_keeps_both_ie_sets() {
        let queue = LinkEventQueue::new();
        for index in 0..LINK_EVENT_QUEUE_DEPTH {
            assert!(queue.enqueue(
                LinkMeta {
                    kind: LINK_EVENT_ASSOCIATE,
                    status_or_reason: index as u16,
                    frequency_mhz: 2437,
                    bssid: [index as u8; 6],
                    first_len: 2,
                    second_len: 3,
                },
                &[1, 2],
                &[3, 4, 5],
            ));
        }
        assert!(!queue.enqueue(
            LinkMeta {
                kind: LINK_EVENT_DISCONNECT,
                status_or_reason: 3,
                frequency_mhz: 0,
                bssid: [0; 6],
                first_len: 0,
                second_len: 0,
            },
            &[],
            &[],
        ));
        assert_eq!(queue.dropped.load(Ordering::Relaxed), 1);
        let event = queue.take_oldest().unwrap();
        assert_eq!(event.meta().status_or_reason, 0);
        assert_eq!(event.first(), &[1, 2]);
        assert_eq!(event.second(), &[3, 4, 5]);
    }

    #[test]
    fn external_auth_queue_is_bounded_and_deep_copies_events() {
        let queue = ExternalAuthQueue::new();
        for index in 0..EXTERNAL_AUTH_QUEUE_DEPTH {
            let mut event = ExternalAuthEvent {
                abi_version: ABI_VERSION,
                action: 0,
                ssid_len: 4,
                bssid: [index as u8; 6],
                status: 0,
                key_mgmt_suite: 0x000f_ac08,
                pmkid_present: 1,
                reserved: [0; 3],
                ssid: [0; 32],
                pmkid: [index as u8; 16],
            };
            event.ssid[..4].copy_from_slice(b"test");
            assert!(queue.enqueue(event));
        }
        assert!(!queue.enqueue(ExternalAuthEvent {
            abi_version: ABI_VERSION,
            action: 1,
            ssid_len: 0,
            bssid: [0; 6],
            status: 1,
            key_mgmt_suite: 0,
            pmkid_present: 0,
            reserved: [0; 3],
            ssid: [0; 32],
            pmkid: [0; 16],
        }));
        assert_eq!(queue.dropped.load(Ordering::Relaxed), 1);

        for index in 0..EXTERNAL_AUTH_QUEUE_DEPTH {
            let event = queue.take_oldest().unwrap();
            let event = event.event();
            assert_eq!(event.bssid, [index as u8; 6]);
            assert_eq!(&event.ssid[..event.ssid_len as usize], b"test");
            assert_eq!(event.pmkid, [index as u8; 16]);
        }
        assert!(!queue.has_pending());
    }

    #[test]
    fn maps_hostap_sae_pwe_to_ws63_uapi_values() {
        assert_eq!(vendor_sae_pwe(SaePwe::HuntAndPeck as u8), Some(1));
        assert_eq!(vendor_sae_pwe(SaePwe::HashToElement as u8), Some(2));
        assert_eq!(vendor_sae_pwe(SaePwe::Both as u8), Some(3));
        assert_eq!(vendor_sae_pwe(3), None);
    }

    #[test]
    fn validates_key_material_and_vendor_cipher_mapping() {
        assert!(valid_key_material(cipher::CCMP, 16));
        assert!(!valid_key_material(cipher::CCMP, 32));
        assert_eq!(cipher_suite(cipher::CCMP, 16), Some(0x000f_ac04));
        assert_eq!(cipher_suite(cipher::BIP_GMAC_256, 32), Some(0x000f_ac0c));
        assert_eq!(cipher_suite(0xff, 16), None);
    }

    #[test]
    fn translates_pairwise_ccmp_key_into_wal_contract() {
        let key = Key {
            abi_version: ABI_VERSION,
            cipher: cipher::CCMP,
            key_index: 0,
            flags: key_flag::RX | key_flag::TX | key_flag::PAIRWISE,
            peer: [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc],
            peer_present: 1,
            sequence_len: 6,
            sequence: [0; ws63_radio_sys::supplicant::KEY_SEQUENCE_LEN],
        };
        let mut material = [0x5a; 16];
        let request = key_request(&key, material.as_mut_ptr(), material.len()).unwrap();

        assert_eq!(request.key_type, KEY_TYPE_PAIRWISE);
        assert_eq!(request.cipher, 0x000f_ac04);
        assert_eq!(request.key_len, 16);
        assert_eq!(request.sequence_len, 6);
        assert_eq!(request.address, key.peer.as_ptr().cast_mut());
        assert_eq!(request.default_data, 1);
        assert_eq!(request.default_management, 0);
        assert_eq!(request.default_types, KEY_DEFAULT_UNICAST);
    }

    #[test]
    fn rejects_ambiguous_or_private_key_flags() {
        let mut key = Key {
            abi_version: ABI_VERSION,
            cipher: cipher::CCMP,
            key_index: 0,
            flags: key_flag::RX | key_flag::TX | key_flag::PAIRWISE | key_flag::GROUP,
            peer: [0; 6],
            peer_present: 1,
            sequence_len: 0,
            sequence: [0; ws63_radio_sys::supplicant::KEY_SEQUENCE_LEN],
        };
        assert!(key_request(&key, core::ptr::null_mut(), 0).is_none());

        key.flags = key_flag::RX | key_flag::TX | key_flag::PAIRWISE | key_flag::PMK;
        assert!(key_request(&key, core::ptr::null_mut(), 0).is_none());
    }
}
