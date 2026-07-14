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
    ABI_VERSION, Context, DriverHooks, Event, Key, OsHooks, PollResult, cipher,
    hisi_wpa_context_align, hisi_wpa_context_size, hisi_wpa_create, hisi_wpa_destroy,
    hisi_wpa_driver_install, hisi_wpa_feed_eapol, hisi_wpa_feed_mgmt, hisi_wpa_init,
    hisi_wpa_next_event, hisi_wpa_os_install, hisi_wpa_os_uninstall, hisi_wpa_poll, key_flag,
};

static RUNNER_WAKE: Semaphore = Semaphore::new(0);
static PORT_IDENTITY: u8 = 0;
static DRIVER_CONTEXT: DriverContext = DriverContext::new();
static PORT_STATE: AtomicU8 = AtomicU8::new(PORT_FREE);
static EAPOL_PENDING: AtomicBool = AtomicBool::new(false);
static MGMT_RX_QUEUE: MgmtRxQueue = MgmtRxQueue::new();

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

const SLOT_FREE: u8 = 0;
const SLOT_WRITING: u8 = 1;
const SLOT_READY: u8 = 2;
const SLOT_READING: u8 = 3;

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

#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(core::mem::size_of::<TxEapol>() == 8);
    assert!(core::mem::size_of::<RxEapol>() == 8);
    assert!(core::mem::size_of::<EnableEapol>() == 8);
    assert!(core::mem::size_of::<MlmeData>() == 16);
    assert!(core::mem::size_of::<KeyExtension>() == 36);
    assert!(core::mem::offset_of!(KeyExtension, address) == 20);
    assert!(core::mem::offset_of!(KeyExtension, default_data) == 32);
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
    /// The WS63 EAPOL receive ioctl or hostap feed rejected a frame.
    FeedEapolFailed(i32),
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
        })
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
        let mut rx_work = false;
        let mut rx_budget = work_budget.get();
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
        if rx_budget != 0 && EAPOL_PENDING.swap(false, Ordering::AcqRel) {
            rx_work |= self.drain_eapol(&mut rx_budget)?;
        }
        // SAFETY: the unique owner serializes all context calls.
        let mut result = unsafe {
            hisi_wpa_poll(
                self.context.as_ptr(),
                crate::uapi::monotonic_ms(),
                work_budget.get(),
            )
        };
        if result.status != 0 {
            return Err(NativeSupplicantError::PollFailed(result.status));
        }
        if rx_work || MGMT_RX_QUEUE.has_pending() || EAPOL_PENDING.load(Ordering::Acquire) {
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
            if status == -22 {
                break;
            }
            if status != 0 {
                return Err(NativeSupplicantError::FeedEapolFailed(status));
            }
            let len = receive.length as usize;
            if len <= ETHERNET_HEADER_LEN
                || len > frame.len()
                || frame[12..ETHERNET_HEADER_LEN] != EAPOL_ETHERTYPE
            {
                return Err(NativeSupplicantError::InvalidResult);
            }
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
        match result {
            0 => Ok(None),
            1 => {
                // SAFETY: a return value of one is the ABI promise that the
                // output was initialized completely.
                let event = unsafe { event.assume_init() };
                if event.abi_version != ABI_VERSION || event.data_len as usize > event.data.len() {
                    Err(NativeSupplicantError::InvalidResult)
                } else {
                    Ok(Some(event))
                }
            }
            status => Err(NativeSupplicantError::PollFailed(status)),
        }
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
        let _ = RUNNER_WAKE.up();
    }
    queued
}

unsafe extern "C" fn eapol_notify(_: *mut c_void, _: *mut c_void) {
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
        send_eapol: Some(send_eapol),
        send_mgmt: Some(send_mgmt),
        install_key: Some(install_key),
        remove_key: Some(remove_key),
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
    crate::wal::ioctl(
        driver.ifname(),
        IOCTL_SEND_EAPOL,
        (&mut request as *mut TxEapol).cast(),
    )
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
    let mut request = MlmeData {
        frequency_mhz,
        data_len: frame_len as u32,
        data: frame.cast_mut(),
        send_action_cookie: driver.send_action_cookie.get(),
    };
    crate::wal::ioctl(
        driver.ifname(),
        IOCTL_SEND_MLME,
        (&mut request as *mut MlmeData).cast(),
    )
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
        use hisi_crypto::EntropySource;

        // SAFETY: null was rejected and the C ABI promises `output_len`
        // writable bytes for the duration of this call.
        let output = unsafe { core::slice::from_raw_parts_mut(output, output_len) };
        crate::crypto::WS63_CRYPTO
            .fill_entropy(output)
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
