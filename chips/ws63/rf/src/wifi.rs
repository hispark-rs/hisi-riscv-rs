//! Thin, safe entry points for the vendor-owned WS63 Wi-Fi runtime.
//!
//! The mask ROM and delivered HMAC/DMAC/TCM blobs retain ownership of the
//! protocol state machines. This module only establishes their documented ABI:
//! initialize the runtime, create one station netdev, issue the vendor scan
//! ioctl, and copy bounded scan events into Rust values.

use core::ffi::c_int;

#[cfg(target_arch = "riscv32")]
use core::cell::{Cell, UnsafeCell};
#[cfg(target_arch = "riscv32")]
use core::ffi::{c_char, c_uint, c_void};
#[cfg(target_arch = "riscv32")]
use critical_section::Mutex;
#[cfg(target_arch = "riscv32")]
use portable_atomic::{AtomicBool, Ordering};

const IFNAME_CAPACITY: usize = 17;
const SSID_CAPACITY: usize = 32;
#[cfg(target_arch = "riscv32")]
const MAX_IE_LENGTH: usize = 2304;

/// Maximum number of access points retained from one scan.
pub const MAX_SCAN_RESULTS: usize = 32;

#[cfg(target_arch = "riscv32")]
const EVENT_SCAN_DONE: c_int = 4;
#[cfg(target_arch = "riscv32")]
const EVENT_SCAN_RESULT: c_int = 5;
#[cfg(target_arch = "riscv32")]
const IOCTL_SCAN: c_uint = 14;
#[cfg(target_arch = "riscv32")]
const IOCTL_SET_NETDEV: c_uint = 17;
#[cfg(target_arch = "riscv32")]
const IFTYPE_STATION: u8 = 2;
#[cfg(target_arch = "riscv32")]
const MODE_11B_G_N_AX: c_uint = 4;

/// Result of the vendor scan operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanStatus {
    /// The driver completed the scan.
    Success,
    /// The driver reported a generic failure.
    Failed,
    /// The driver refused to start or continue the scan.
    Refused,
    /// The driver timed out internally.
    Timeout,
    /// A newer vendor runtime returned a status this crate does not know yet.
    Unknown(u32),
}

impl ScanStatus {
    #[cfg(target_arch = "riscv32")]
    const fn from_raw(raw: u32) -> Self {
        match raw {
            0 => Self::Success,
            1 => Self::Failed,
            2 => Self::Refused,
            3 => Self::Timeout,
            value => Self::Unknown(value),
        }
    }
}

/// One bounded scan result copied out of the vendor event buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanResult {
    ssid: [u8; SSID_CAPACITY],
    ssid_len: u8,
    /// Basic service set identifier.
    pub bssid: [u8; 6],
    /// Center frequency in MHz.
    pub frequency_mhz: u16,
    /// Signal strength in dBm. The vendor ABI reports hundredths of a dBm.
    pub rssi_dbm: i16,
}

impl ScanResult {
    const EMPTY: Self = Self {
        ssid: [0; SSID_CAPACITY],
        ssid_len: 0,
        bssid: [0; 6],
        frequency_mhz: 0,
        rssi_dbm: 0,
    };

    /// SSID bytes exactly as advertised by the access point.
    pub fn ssid(&self) -> &[u8] {
        &self.ssid[..self.ssid_len as usize]
    }

    /// Empty value for caller-provided scan buffers.
    pub const fn empty() -> Self {
        Self::EMPTY
    }
}

/// Error returned by the thin Wi-Fi adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The single vendor Wi-Fi runtime was already claimed.
    AlreadyInitialized,
    /// `uapi_wifi_init` failed with the enclosed vendor error code.
    Initialize(u32),
    /// The vendor WAL failed to create the station netdev.
    CreateStation(c_int),
    /// Another callback already owns the vendor event channel.
    RegisterEvents(c_int),
    /// The vendor refused to open the station netdev.
    OpenStation(c_int),
    /// A scan is already in progress.
    Busy,
    /// The vendor scan ioctl failed.
    StartScan(c_int),
    /// The scan finished with a non-success vendor status.
    ScanFailed(ScanStatus),
    /// Rust stopped waiting before the vendor emitted scan-done.
    Timeout,
    /// This API only runs on the WS63 RISC-V target.
    UnsupportedTarget,
}

/// Exclusive handle to the vendor-owned WS63 Wi-Fi runtime.
pub struct Wifi {
    ifname: [u8; IFNAME_CAPACITY],
}

impl Wifi {
    /// Initialize the ROM/blob runtime and create its station network device.
    ///
    /// This is a one-shot operation. Once the vendor runtime has started, a
    /// partial failure cannot be rolled back safely, so later calls return
    /// [`Error::AlreadyInitialized`].
    pub fn initialize() -> Result<Self, Error> {
        #[cfg(not(target_arch = "riscv32"))]
        {
            Err(Error::UnsupportedTarget)
        }

        #[cfg(target_arch = "riscv32")]
        {
            if WIFI_CLAIMED
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return Err(Error::AlreadyInitialized);
            }

            crate::force_link_contract();
            // SAFETY: the one-shot claim above guarantees this runs once,
            // before the vendor stack can access its dedicated RAM windows.
            unsafe { crate::prepare_vendor_memory() };

            // SAFETY: the RF build links the matching WS63 vendor archives and
            // ROM symbol table; the Rust OSAL contract has been installed.
            let init = unsafe { uapi_wifi_init(2, 7) };
            if init != 0 {
                return Err(Error::Initialize(init));
            }

            let mut ifname = [0_u8; IFNAME_CAPACITY];
            let mut length = (IFNAME_CAPACITY - 1) as u32;
            // SAFETY: `ifname` is writable for `length + 1` bytes and remains
            // alive for the call. The enum values match the delivered headers.
            let create = unsafe {
                wal_init_drv_wlan_netdev(
                    IFTYPE_STATION,
                    MODE_11B_G_N_AX,
                    ifname.as_mut_ptr().cast(),
                    &mut length,
                )
            };
            if create != 0 {
                return Err(Error::CreateStation(create));
            }
            if length == 0 || length as usize >= IFNAME_CAPACITY || ifname[length as usize] != 0 {
                return Err(Error::CreateStation(-1));
            }

            // SAFETY: the callback has the exact vendor C ABI and remains
            // installed for the process lifetime. It never calls user code.
            let register = unsafe { drv_soc_register_send_event_cb(Some(scan_event)) };
            if register != 0 {
                return Err(Error::RegisterEvents(register));
            }

            // This mirrors the only control-plane step from the vendor
            // `drv_soc_wpa_init` needed before scan. WPA/eloop/EAPOL stay out
            // of this scan-only adapter.
            let mut enabled = 1_u8;
            let command = VendorIoctl {
                command: IOCTL_SET_NETDEV,
                buffer: (&mut enabled as *mut u8).cast(),
            };
            // SAFETY: the interface name and one-byte boolean remain valid for
            // the synchronous vendor ioctl.
            let open = unsafe { drv_soc_hwal_wpa_ioctl(ifname.as_mut_ptr().cast(), &command) };
            if open != 0 {
                return Err(Error::OpenStation(open));
            }

            Ok(Self { ifname })
        }
    }

    /// Perform an untargeted station scan.
    ///
    /// Results are copied into `output` after the vendor scan-done event. At
    /// most [`MAX_SCAN_RESULTS`] are retained; excess events are deliberately
    /// dropped. No user callback executes in the ROM/HMAC event context.
    pub fn scan(&mut self, output: &mut [ScanResult], timeout_ms: u32) -> Result<usize, Error> {
        #[cfg(not(target_arch = "riscv32"))]
        {
            let _ = (output, timeout_ms);
            Err(Error::UnsupportedTarget)
        }

        #[cfg(target_arch = "riscv32")]
        {
            let started = critical_section::with(|cs| {
                let state = SCAN_STATE.borrow(cs);
                if state.active.get() {
                    return false;
                }
                state.active.set(true);
                state.done.set(false);
                state.count.set(0);
                state.status.set(ScanStatus::Success);
                true
            });
            if !started {
                return Err(Error::Busy);
            }

            let mut wildcard = VendorScanSsid::zeroed();
            let mut params = VendorScan {
                ssids: &mut wildcard,
                frequencies: core::ptr::null_mut(),
                extra_ies: core::ptr::null_mut(),
                bssid: core::ptr::null_mut(),
                num_ssids: 1,
                num_frequencies: 0,
                prefix_ssid: 0,
                fast_connect: 0,
                extra_ies_len: 0,
            };
            let command = VendorIoctl {
                command: IOCTL_SCAN,
                buffer: (&mut params as *mut VendorScan).cast(),
            };

            // SAFETY: the interface name is NUL-terminated and owned by self;
            // official code also frees scan parameters immediately after this
            // synchronous ioctl returns.
            let result =
                unsafe { drv_soc_hwal_wpa_ioctl(self.ifname.as_mut_ptr().cast(), &command) };
            if result != 0 {
                finish_scan();
                return Err(Error::StartScan(result));
            }

            let started_at = crate::uapi::uapi_systick_get_ms();
            loop {
                let (done, status, count) = critical_section::with(|cs| {
                    let state = SCAN_STATE.borrow(cs);
                    (state.done.get(), state.status.get(), state.count.get())
                });
                if done {
                    finish_scan();
                    if status != ScanStatus::Success {
                        return Err(Error::ScanFailed(status));
                    }
                    let copy_len = count.min(output.len());
                    // SAFETY: scan-done is emitted after result callbacks. The
                    // event callback no longer writes these initialized slots.
                    unsafe {
                        let stored = &*SCAN_RESULTS.0.get();
                        output[..copy_len].copy_from_slice(&stored[..copy_len]);
                    }
                    return Ok(copy_len);
                }
                if crate::uapi::uapi_systick_get_ms().wrapping_sub(started_at) >= timeout_ms as u64
                {
                    finish_scan();
                    return Err(Error::Timeout);
                }
                crate::sched::sleep_ms(1);
            }
        }
    }

    /// Vendor-created, NUL-free interface name.
    pub fn interface_name(&self) -> &[u8] {
        let len = self
            .ifname
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(self.ifname.len());
        &self.ifname[..len]
    }
}

#[cfg(target_arch = "riscv32")]
#[repr(C)]
struct VendorScanSsid {
    ssid: [u8; SSID_CAPACITY],
    ssid_len: u32,
}

#[cfg(target_arch = "riscv32")]
impl VendorScanSsid {
    const fn zeroed() -> Self {
        Self {
            ssid: [0; SSID_CAPACITY],
            ssid_len: 0,
        }
    }
}

#[cfg(target_arch = "riscv32")]
#[repr(C)]
struct VendorScan {
    ssids: *mut VendorScanSsid,
    frequencies: *mut i32,
    extra_ies: *mut u8,
    bssid: *mut u8,
    num_ssids: u8,
    num_frequencies: u8,
    prefix_ssid: u8,
    fast_connect: u8,
    extra_ies_len: u32,
}

#[cfg(target_arch = "riscv32")]
#[repr(C)]
struct VendorIoctl {
    command: c_uint,
    buffer: *mut c_void,
}

#[cfg(target_arch = "riscv32")]
#[repr(C)]
struct VendorScanResult {
    flags: i32,
    bssid: [u8; 6],
    capabilities: i16,
    frequency: i32,
    beacon_interval: i16,
    quality: i32,
    beacon_ie_len: u32,
    level: i32,
    age: u32,
    ie_len: u32,
    variable: *const u8,
}

#[cfg(target_arch = "riscv32")]
const _: () = {
    assert!(core::mem::size_of::<VendorScanSsid>() == 36);
    assert!(core::mem::size_of::<VendorScan>() == 24);
    assert!(core::mem::size_of::<VendorIoctl>() == 8);
    assert!(core::mem::size_of::<VendorScanResult>() == 44);
};

#[cfg(target_arch = "riscv32")]
struct ScanState {
    active: Cell<bool>,
    done: Cell<bool>,
    count: Cell<usize>,
    status: Cell<ScanStatus>,
}

#[cfg(target_arch = "riscv32")]
static SCAN_STATE: Mutex<ScanState> = Mutex::new(ScanState {
    active: Cell::new(false),
    done: Cell::new(false),
    count: Cell::new(0),
    status: Cell::new(ScanStatus::Success),
});

#[cfg(target_arch = "riscv32")]
struct ScanResultStorage(UnsafeCell<[ScanResult; MAX_SCAN_RESULTS]>);

#[cfg(target_arch = "riscv32")]
// SAFETY: slots are reserved under `SCAN_STATE`; the cooperative event callback
// completes each write before scan-done makes the array visible to the waiter.
unsafe impl Sync for ScanResultStorage {}

#[cfg(target_arch = "riscv32")]
static SCAN_RESULTS: ScanResultStorage =
    ScanResultStorage(UnsafeCell::new([ScanResult::EMPTY; MAX_SCAN_RESULTS]));

#[cfg(target_arch = "riscv32")]
static WIFI_CLAIMED: AtomicBool = AtomicBool::new(false);

#[cfg(target_arch = "riscv32")]
fn finish_scan() {
    critical_section::with(|cs| SCAN_STATE.borrow(cs).active.set(false));
}

#[cfg(any(target_arch = "riscv32", test))]
fn ssid_from_ies(ies: &[u8]) -> &[u8] {
    let mut offset = 0;
    while offset + 2 <= ies.len() {
        let id = ies[offset];
        let len = ies[offset + 1] as usize;
        let end = offset + 2 + len;
        if end > ies.len() {
            return &[];
        }
        if id == 0 {
            return &ies[offset + 2..end.min(offset + 2 + SSID_CAPACITY)];
        }
        offset = end;
    }
    &[]
}

#[cfg(target_arch = "riscv32")]
unsafe extern "C" fn scan_event(
    _ifname: *const c_char,
    event: c_int,
    data: *mut u8,
    length: c_uint,
) -> c_int {
    if event == EVENT_SCAN_RESULT
        && !data.is_null()
        && length as usize == core::mem::size_of::<VendorScanResult>()
    {
        let slot = critical_section::with(|cs| {
            let state = SCAN_STATE.borrow(cs);
            if !state.active.get() || state.count.get() >= MAX_SCAN_RESULTS {
                return None;
            }
            let slot = state.count.get();
            state.count.set(slot + 1);
            Some(slot)
        });
        if let Some(slot) = slot {
            // SAFETY: the callback ABI guarantees a live, aligned result for
            // the duration of this call; pointer fields are copied immediately.
            let vendor = unsafe { &*data.cast::<VendorScanResult>() };
            let mut result = ScanResult::EMPTY;
            result.bssid = vendor.bssid;
            result.frequency_mhz = vendor.frequency.clamp(0, u16::MAX as i32) as u16;
            result.rssi_dbm = (vendor.level / 100).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            if !vendor.variable.is_null() && vendor.ie_len as usize <= MAX_IE_LENGTH {
                // SAFETY: the vendor event owns a readable IE buffer for this
                // callback and reports its exact byte length.
                let ies =
                    unsafe { core::slice::from_raw_parts(vendor.variable, vendor.ie_len as usize) };
                let ssid = ssid_from_ies(ies);
                result.ssid[..ssid.len()].copy_from_slice(ssid);
                result.ssid_len = ssid.len() as u8;
            }
            // SAFETY: this callback exclusively owns the reserved slot until
            // scan-done; the waiter only reads after observing scan-done.
            unsafe { (*SCAN_RESULTS.0.get())[slot] = result };
        }
    } else if event == EVENT_SCAN_DONE && !data.is_null() && length >= 1 {
        // The delivered object stores `ext_scan_status_enum` with `sb` and
        // reports a one-byte payload (`-fshort-enums` vendor ABI).
        // SAFETY: the callback reports at least one readable status byte.
        let raw = unsafe { data.read() } as u32;
        critical_section::with(|cs| {
            let state = SCAN_STATE.borrow(cs);
            if state.active.get() {
                state.status.set(ScanStatus::from_raw(raw));
                state.done.set(true);
            }
        });
    }
    0
}

#[cfg(target_arch = "riscv32")]
unsafe extern "C" {
    fn uapi_wifi_init(vap_res_num: u8, user_res_num: u8) -> u32;
    fn wal_init_drv_wlan_netdev(
        interface_type: u8,
        mode: c_uint,
        ifname: *mut c_char,
        length: *mut u32,
    ) -> c_int;
    fn drv_soc_register_send_event_cb(
        callback: Option<unsafe extern "C" fn(*const c_char, c_int, *mut u8, c_uint) -> c_int>,
    ) -> c_int;
    fn drv_soc_hwal_wpa_ioctl(ifname: *mut c_char, command: *const VendorIoctl) -> c_int;
}

#[cfg(test)]
mod tests {
    use super::ssid_from_ies;

    #[test]
    fn finds_ssid_information_element() {
        assert_eq!(ssid_from_ies(&[1, 1, 0x82, 0, 3, b'a', b'p', b'1']), b"ap1");
    }

    #[test]
    fn rejects_truncated_information_element() {
        assert_eq!(ssid_from_ies(&[0, 4, b'a', b'b']), b"");
    }
}
