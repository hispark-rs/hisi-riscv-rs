//! Thin, safe entry points for the vendor-owned WS63 Wi-Fi runtime.
//!
//! The mask ROM and delivered HMAC/DMAC/TCM blobs retain ownership of the
//! protocol state machines. This module only establishes their documented ABI:
//! initialize the runtime, create one station netdev, issue the vendor scan
//! ioctl, and copy bounded scan events into Rust values.

use core::ffi::c_int;
use hisi_riscv_hal::peripherals::Efuse;

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
const WPA_KEY_CAPACITY: usize = 64;
#[cfg(target_arch = "riscv32")]
const MAX_IE_LENGTH: usize = 2304;

/// Maximum number of access points retained from one scan.
pub const MAX_SCAN_RESULTS: usize = 32;

#[cfg(target_arch = "riscv32")]
const EVENT_SCAN_DONE: c_int = 4;
#[cfg(target_arch = "riscv32")]
const EVENT_SCAN_RESULT: c_int = 5;
#[cfg(target_arch = "riscv32")]
const EVENT_CONNECT_RESULT: c_int = 6;
#[cfg(target_arch = "riscv32")]
const EVENT_DISCONNECT: c_int = 7;
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
    security: ScanSecurity,
    auth_mode: i32,
    pairwise: i32,
    channel: u8,
}

/// Security classification available from the scan result's capability bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanSecurity {
    /// IEEE 802.11 privacy is not advertised; no link-layer key is required.
    Open,
    /// Privacy is advertised. WPA/WPA2/WPA3 details require parsing the IEs.
    Protected,
}

/// A discovered open network that can be passed to [`Wifi::connect_open`].
///
/// This type deliberately has no password or security-mode fields. RF5B only
/// proves the unencrypted association and L2 data paths; authenticated networks
/// will use a separate configuration type once the WPA boundary is integrated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenNetwork {
    ssid: [u8; SSID_CAPACITY],
    ssid_len: u8,
    bssid: [u8; 6],
    frequency_mhz: u16,
}

/// A discovered WPA2-Personal/CCMP network and validated ASCII passphrase.
#[cfg(feature = "wifi-wpa2-personal")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersonalNetwork {
    ssid: [u8; SSID_CAPACITY],
    ssid_len: u8,
    bssid: [u8; 6],
    auth_mode: i32,
    pairwise: i32,
    channel: u8,
    key: [u8; WPA_KEY_CAPACITY + 1],
    key_len: u8,
}

#[cfg(feature = "wifi-wpa2-personal")]
impl PersonalNetwork {
    /// Select a WPA2-Personal/CCMP AP and validate an ASCII passphrase.
    pub fn from_scan(result: &ScanResult, passphrase: &[u8]) -> Result<Self, Error> {
        if result.ssid_len == 0 {
            return Err(Error::InvalidSsid);
        }
        if result.security != ScanSecurity::Protected {
            return Err(Error::OpenNetwork);
        }
        // Delivered SDK enums: WPA2PSK=2 and AES/CCMP=1. WPA1 mixed mode,
        // SAE/transition mode, Enterprise and TKIP remain separate gates.
        if result.auth_mode != 2 || result.pairwise != 1 {
            return Err(Error::UnsupportedSecurity(result.auth_mode));
        }
        if !(8..=63).contains(&passphrase.len())
            || passphrase.iter().any(|byte| *byte < 32 || *byte == 127)
        {
            return Err(Error::InvalidPassphrase);
        }
        let mut key = [0; WPA_KEY_CAPACITY + 1];
        key[..passphrase.len()].copy_from_slice(passphrase);
        Ok(Self {
            ssid: result.ssid,
            ssid_len: result.ssid_len,
            bssid: result.bssid,
            auth_mode: result.auth_mode,
            pairwise: result.pairwise,
            channel: result.channel,
            key,
            key_len: passphrase.len() as u8,
        })
    }

    /// SSID bytes used for this connection.
    pub fn ssid(&self) -> &[u8] {
        &self.ssid[..self.ssid_len as usize]
    }
}

impl OpenNetwork {
    /// Select an open network from a scan result.
    pub fn from_scan(result: &ScanResult) -> Result<Self, Error> {
        if result.ssid_len == 0 {
            return Err(Error::InvalidSsid);
        }
        if result.security != ScanSecurity::Open {
            return Err(Error::ProtectedNetwork);
        }
        Ok(Self {
            ssid: result.ssid,
            ssid_len: result.ssid_len,
            bssid: result.bssid,
            frequency_mhz: result.frequency_mhz,
        })
    }

    /// SSID bytes used for this association.
    pub fn ssid(&self) -> &[u8] {
        &self.ssid[..self.ssid_len as usize]
    }
}

/// Successful station association reported by the vendor runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionInfo {
    /// BSSID selected by the firmware.
    pub bssid: [u8; 6],
    /// Associated center frequency in MHz.
    pub frequency_mhz: u16,
}

impl ScanResult {
    const EMPTY: Self = Self {
        ssid: [0; SSID_CAPACITY],
        ssid_len: 0,
        bssid: [0; 6],
        frequency_mhz: 0,
        rssi_dbm: 0,
        security: ScanSecurity::Open,
        auth_mode: 0,
        pairwise: 0,
        channel: 0,
    };

    /// SSID bytes exactly as advertised by the access point.
    pub fn ssid(&self) -> &[u8] {
        &self.ssid[..self.ssid_len as usize]
    }

    /// Empty value for caller-provided scan buffers.
    pub const fn empty() -> Self {
        Self::EMPTY
    }

    /// Coarse security classification reported by this beacon/probe response.
    pub const fn security(&self) -> ScanSecurity {
        self.security
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
    /// The mask-ROM TCXO driver failed to initialize.
    Timebase(u32),
    /// A scan is already in progress.
    Busy,
    /// The selected network has an empty or otherwise unusable SSID.
    InvalidSsid,
    /// The selected AP advertises link-layer privacy and is not an open network.
    ProtectedNetwork,
    /// The selected AP is open and cannot be used as a WPA personal network.
    OpenNetwork,
    /// The scan reported a security mode this adapter does not support yet.
    UnsupportedSecurity(i32),
    /// WPA personal passphrases must be 8-63 printable ASCII bytes.
    InvalidPassphrase,
    /// The vendor scan ioctl failed.
    StartScan(c_int),
    /// The scan finished with a non-success vendor status.
    ScanFailed(ScanStatus),
    /// The vendor refused to start the association.
    StartConnect(c_int),
    /// Association completed with an IEEE 802.11 status other than success.
    ConnectFailed(u16),
    /// The station disconnected while association was pending.
    Disconnected(u16),
    /// Rust stopped waiting before the vendor emitted scan-done.
    Timeout,
    /// This API only runs on the WS63 RISC-V target.
    UnsupportedTarget,
}

/// Exclusive handle to the vendor-owned WS63 Wi-Fi runtime.
pub struct Wifi<'d> {
    ifname: [u8; IFNAME_CAPACITY],
    _efuse: Efuse<'d>,
}

/// Exclusive station handle backed by the vendor WPA supplicant.
#[cfg(feature = "wifi-wpa2-personal")]
pub struct WpaWifi<'d> {
    ifname: [u8; IFNAME_CAPACITY],
    _efuse: Efuse<'d>,
}

#[cfg(feature = "wifi-wpa2-personal")]
impl<'d> WpaWifi<'d> {
    /// Initialize the RF runtime, start the STA interface and its supplicant task.
    pub fn initialize(efuse: Efuse<'d>) -> Result<Self, Error> {
        #[cfg(not(target_arch = "riscv32"))]
        {
            let _ = efuse;
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
            unsafe { crate::prepare_vendor_memory() };
            let timebase = crate::uapi::initialize_rom_timebases();
            if timebase != 0 {
                return Err(Error::Timebase(timebase));
            }
            crate::uapi::enable_efuse_reads();
            // Match the vendor startup order: initialize the unified cipher
            // driver, then register its hash/AES/ECP mbedTLS providers before
            // the supplicant can derive a PSK or process EAPOL frames.
            unsafe { uapi_drv_cipher_env_init() };
            let security = unsafe { mbedtls_adapt_register_func() };
            if security != 0 {
                return Err(Error::Initialize(security as u32));
            }
            let init = unsafe { uapi_wifi_init(2, 7) };
            if init != 0 {
                return Err(Error::Initialize(init));
            }
            let mut ifname = [0; IFNAME_CAPACITY];
            let mut length = IFNAME_CAPACITY as c_int;
            let start = unsafe { uapi_wifi_sta_start(ifname.as_mut_ptr().cast(), &mut length) };
            if start != 0 || length <= 0 || length as usize >= IFNAME_CAPACITY {
                return Err(Error::CreateStation(start));
            }
            let callback_mode = unsafe { uapi_wifi_config_callback(1, 10, 2048) };
            if callback_mode != 0 {
                return Err(Error::RegisterEvents(callback_mode));
            }
            let register = unsafe { uapi_wifi_register_event_callback(Some(wpa_event)) };
            if register != 0 {
                return Err(Error::RegisterEvents(register));
            }
            #[cfg(feature = "net")]
            crate::netif_smoltcp::set_tx_sink(crate::netif::vendor_tx_sink);
            Ok(Self {
                ifname,
                _efuse: efuse,
            })
        }
    }

    /// Scan using the supplicant-owned event channel and copy bounded results.
    pub fn scan(&mut self, output: &mut [ScanResult], timeout_ms: u32) -> Result<usize, Error> {
        #[cfg(not(target_arch = "riscv32"))]
        {
            let _ = (output, timeout_ms);
            Err(Error::UnsupportedTarget)
        }
        #[cfg(target_arch = "riscv32")]
        {
            if unsafe { uapi_wifi_sta_scan() } != 0 {
                return Err(Error::StartScan(-1));
            }
            let started_at = crate::uapi::monotonic_ms();
            while unsafe { uapi_wifi_get_scan_flag() } != 0 {
                if crate::uapi::monotonic_ms().wrapping_sub(started_at) >= timeout_ms as u64 {
                    return Err(Error::Timeout);
                }
                crate::sched::sleep_ms(1);
            }
            let mut vendor = [VendorWpaApInfo::zeroed(); MAX_SCAN_RESULTS];
            let mut count = MAX_SCAN_RESULTS as c_uint;
            if unsafe { uapi_wifi_get_scan_results(vendor.as_mut_ptr(), &mut count) } != 0 {
                return Err(Error::ScanFailed(ScanStatus::Failed));
            }
            let count = (count as usize).min(output.len()).min(MAX_SCAN_RESULTS);
            for (dst, src) in output[..count].iter_mut().zip(&vendor[..count]) {
                let ssid_len = src
                    .ssid
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(SSID_CAPACITY);
                let mut result = ScanResult::EMPTY;
                result.ssid[..ssid_len].copy_from_slice(&src.ssid[..ssid_len]);
                result.ssid_len = ssid_len as u8;
                result.bssid = src.bssid;
                result.channel = src.channel.clamp(0, u8::MAX as u32) as u8;
                result.frequency_mhz = channel_to_frequency(result.channel);
                result.rssi_dbm = (src.rssi / 100).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                result.auth_mode = i32::from(src.auth_mode);
                result.pairwise = i32::from(src.pairwise);
                result.security = if src.auth_mode == 0 {
                    ScanSecurity::Open
                } else {
                    ScanSecurity::Protected
                };
                *dst = result;
            }
            Ok(count)
        }
    }

    /// Connect and wait until the WPA supplicant reports an authorized link.
    pub fn connect(
        &mut self,
        network: &PersonalNetwork,
        timeout_ms: u32,
    ) -> Result<ConnectionInfo, Error> {
        #[cfg(not(target_arch = "riscv32"))]
        {
            let _ = (network, timeout_ms);
            Err(Error::UnsupportedTarget)
        }
        #[cfg(target_arch = "riscv32")]
        {
            let started = critical_section::with(|cs| {
                let state = CONNECTION_STATE.borrow(cs);
                if state.active.get() {
                    return false;
                }
                state.active.set(true);
                state.done.set(false);
                state.outcome.set(ConnectionOutcome::Pending);
                true
            });
            if !started {
                return Err(Error::Busy);
            }
            let mut request = VendorWpaAssoc::zeroed();
            request.ssid[..network.ssid_len as usize]
                .copy_from_slice(&network.ssid[..network.ssid_len as usize]);
            request.auth_mode = network.auth_mode as u8;
            request.key[..network.key_len as usize]
                .copy_from_slice(&network.key[..network.key_len as usize]);
            // Leave BSSID unspecified. The delivered control path formats a
            // pinned BSSID through a six-argument `snprintf_s(MACSTR, ...)`;
            // the minimal Rust libc adapter intentionally does not emulate
            // arbitrary C variadics. SSID selection is an official API path.
            request.pairwise = network.pairwise as u8;
            request.channel = network.channel;
            let result = unsafe { uapi_wifi_sta_connect(&request) };
            if result != 0 {
                finish_connection();
                return Err(Error::StartConnect(result));
            }
            let started_at = crate::uapi::monotonic_ms();
            loop {
                let (done, outcome) = critical_section::with(|cs| {
                    let state = CONNECTION_STATE.borrow(cs);
                    (state.done.get(), state.outcome.get())
                });
                if done {
                    finish_connection();
                    return match outcome {
                        ConnectionOutcome::Connected(info) => Ok(ConnectionInfo {
                            frequency_mhz: channel_to_frequency(network.channel),
                            ..info
                        }),
                        ConnectionOutcome::Failed(status) => Err(Error::ConnectFailed(status)),
                        ConnectionOutcome::Disconnected(reason) => Err(Error::Disconnected(reason)),
                        ConnectionOutcome::Pending => Err(Error::Timeout),
                    };
                }
                if crate::uapi::monotonic_ms().wrapping_sub(started_at) >= timeout_ms as u64 {
                    finish_connection();
                    return Err(Error::Timeout);
                }
                crate::sched::sleep_ms(10);
            }
        }
    }

    /// Vendor-created interface name.
    pub fn interface_name(&self) -> &[u8] {
        let len = self
            .ifname
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(IFNAME_CAPACITY);
        &self.ifname[..len]
    }
}

#[cfg(feature = "wifi-wpa2-personal")]
const fn channel_to_frequency(channel: u8) -> u16 {
    if channel == 14 {
        2484
    } else if channel >= 1 && channel <= 13 {
        2407 + channel as u16 * 5
    } else {
        0
    }
}

impl<'d> Wifi<'d> {
    /// Initialize the ROM/blob runtime and create its station network device.
    ///
    /// This is a one-shot operation. Once the vendor runtime has started, a
    /// partial failure cannot be rolled back safely, so later calls return
    /// [`Error::AlreadyInitialized`].
    pub fn initialize(efuse: Efuse<'d>) -> Result<Self, Error> {
        #[cfg(not(target_arch = "riscv32"))]
        {
            let _ = efuse;
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
            let timebase = crate::uapi::initialize_rom_timebases();
            if timebase != 0 {
                return Err(Error::Timebase(timebase));
            }
            crate::uapi::enable_efuse_reads();

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

            #[cfg(feature = "net")]
            crate::netif_smoltcp::set_tx_sink(crate::netif::vendor_tx_sink);

            Ok(Self {
                ifname,
                _efuse: efuse,
            })
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

            let started_at = crate::uapi::monotonic_ms();
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
                if crate::uapi::monotonic_ms().wrapping_sub(started_at) >= timeout_ms as u64 {
                    finish_scan();
                    return Err(Error::Timeout);
                }
                crate::sched::sleep_ms(1);
            }
        }
    }

    /// Associate with a discovered unencrypted access point.
    ///
    /// The request is copied by the synchronous vendor ioctl. Completion is
    /// deferred through the shared event callback and observed here in normal
    /// task context; no user callback runs inside the vendor event path.
    pub fn connect_open(
        &mut self,
        network: &OpenNetwork,
        timeout_ms: u32,
    ) -> Result<ConnectionInfo, Error> {
        #[cfg(not(target_arch = "riscv32"))]
        {
            let _ = (network, timeout_ms);
            Err(Error::UnsupportedTarget)
        }

        #[cfg(target_arch = "riscv32")]
        {
            let started = critical_section::with(|cs| {
                let state = CONNECTION_STATE.borrow(cs);
                if state.active.get() {
                    return false;
                }
                state.active.set(true);
                state.done.set(false);
                state.outcome.set(ConnectionOutcome::Pending);
                true
            });
            if !started {
                return Err(Error::Busy);
            }

            let mut ssid = network.ssid;
            let mut bssid = network.bssid;
            let mut crypto = VendorCryptoSettings::zeroed();
            let mut params = VendorAssociateParams {
                bssid: bssid.as_mut_ptr(),
                ssid: ssid.as_mut_ptr(),
                ie: core::ptr::null_mut(),
                key: core::ptr::null_mut(),
                auth_type: 0, // EXT_AUTHTYPE_OPEN_SYSTEM
                privacy: 0,
                key_len: 0,
                key_idx: 0,
                mfp: 0,
                auto_connect: 0,
                reserved: [0; 2],
                frequency_mhz: network.frequency_mhz as u32,
                ssid_len: network.ssid_len as u32,
                ie_len: 0,
                crypto: &mut crypto,
            };

            // SAFETY: all pointers in `params` refer to live local buffers for
            // the synchronous call. The layout is asserted against the SDK's
            // RV32 `ext_associate_params_stru` contract below.
            let result = unsafe {
                uapi_ioctl_assoc(
                    self.ifname.as_ptr().cast(),
                    (&mut params as *mut VendorAssociateParams).cast(),
                )
            };
            if result != 0 {
                finish_connection();
                return Err(Error::StartConnect(result));
            }

            let started_at = crate::uapi::monotonic_ms();
            loop {
                let (done, outcome) = critical_section::with(|cs| {
                    let state = CONNECTION_STATE.borrow(cs);
                    (state.done.get(), state.outcome.get())
                });
                if done {
                    finish_connection();
                    return match outcome {
                        ConnectionOutcome::Connected(info) => Ok(info),
                        ConnectionOutcome::Failed(status) => Err(Error::ConnectFailed(status)),
                        ConnectionOutcome::Disconnected(reason) => Err(Error::Disconnected(reason)),
                        ConnectionOutcome::Pending => Err(Error::Timeout),
                    };
                }
                if crate::uapi::monotonic_ms().wrapping_sub(started_at) >= timeout_ms as u64 {
                    finish_connection();
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
#[repr(C)]
struct VendorCryptoSettings {
    wpa_versions: u32,
    cipher_group: u32,
    pairwise_count: i32,
    pairwise: [u32; 5],
    akm_count: i32,
    akm: [u32; 2],
    sae_pwe: u8,
    reserved: [u8; 3],
}

#[cfg(target_arch = "riscv32")]
impl VendorCryptoSettings {
    const fn zeroed() -> Self {
        Self {
            wpa_versions: 0,
            cipher_group: 0,
            pairwise_count: 0,
            pairwise: [0; 5],
            akm_count: 0,
            akm: [0; 2],
            sae_pwe: 0,
            reserved: [0; 3],
        }
    }
}

#[cfg(target_arch = "riscv32")]
#[repr(C)]
struct VendorAssociateParams {
    bssid: *mut u8,
    ssid: *mut u8,
    ie: *mut u8,
    key: *mut u8,
    auth_type: u8,
    privacy: u8,
    key_len: u8,
    key_idx: u8,
    mfp: u8,
    auto_connect: u8,
    reserved: [u8; 2],
    frequency_mhz: u32,
    ssid_len: u32,
    ie_len: u32,
    crypto: *mut VendorCryptoSettings,
}

#[cfg(target_arch = "riscv32")]
#[repr(C)]
struct VendorConnectResult {
    request_ie: *mut u8,
    request_ie_len: u32,
    response_ie: *mut u8,
    response_ie_len: u32,
    bssid: [u8; 6],
    reserved: [u8; 2],
    status: u16,
    frequency_mhz: u16,
}

#[cfg(target_arch = "riscv32")]
#[repr(C)]
struct VendorDisconnect {
    ie: *mut u8,
    reason: u16,
    reserved: [u8; 2],
    ie_len: u32,
}

#[cfg(all(feature = "wifi-wpa2-personal", target_arch = "riscv32"))]
#[derive(Clone, Copy)]
#[repr(C)]
struct VendorWpaApInfo {
    ssid: [u8; SSID_CAPACITY + 1],
    bssid: [u8; 6],
    auth_mode: u8,
    channel: u32,
    rssi: i32,
    flags: u8,
    pairwise: u8,
    tail_padding: [u8; 2],
}

#[cfg(all(feature = "wifi-wpa2-personal", target_arch = "riscv32"))]
impl VendorWpaApInfo {
    const fn zeroed() -> Self {
        Self {
            ssid: [0; SSID_CAPACITY + 1],
            bssid: [0; 6],
            auth_mode: 0,
            channel: 0,
            rssi: 0,
            flags: 0,
            pairwise: 0,
            tail_padding: [0; 2],
        }
    }
}

#[cfg(all(feature = "wifi-wpa2-personal", target_arch = "riscv32"))]
#[repr(C)]
struct VendorWpaAssoc {
    ssid: [u8; SSID_CAPACITY + 1],
    auth_mode: u8,
    key: [u8; WPA_KEY_CAPACITY + 1],
    bssid: [u8; 6],
    pairwise: u8,
    hex_flag: u8,
    ft_flag: u8,
    channel: u8,
    reserved: [u8; 2],
}

#[cfg(all(feature = "wifi-wpa2-personal", target_arch = "riscv32"))]
impl VendorWpaAssoc {
    const fn zeroed() -> Self {
        Self {
            ssid: [0; SSID_CAPACITY + 1],
            auth_mode: 0,
            key: [0; WPA_KEY_CAPACITY + 1],
            bssid: [0; 6],
            pairwise: 0,
            hex_flag: 0,
            ft_flag: 0,
            channel: 0,
            reserved: [0; 2],
        }
    }
}

#[cfg(all(feature = "wifi-wpa2-personal", target_arch = "riscv32"))]
#[repr(C)]
struct VendorWpaEvent {
    kind: u8,
    padding: [u8; 3],
    info: [u8; 172],
}

#[cfg(target_arch = "riscv32")]
const _: () = {
    assert!(core::mem::size_of::<VendorScanSsid>() == 36);
    assert!(core::mem::size_of::<VendorScan>() == 24);
    assert!(core::mem::size_of::<VendorIoctl>() == 8);
    assert!(core::mem::size_of::<VendorScanResult>() == 44);
    assert!(core::mem::size_of::<VendorCryptoSettings>() == 48);
    assert!(core::mem::size_of::<VendorAssociateParams>() == 40);
    assert!(core::mem::size_of::<VendorConnectResult>() == 28);
    assert!(core::mem::size_of::<VendorDisconnect>() == 12);
    #[cfg(feature = "wifi-wpa2-personal")]
    {
        // The vendor SDK compiles these public C structs with -fshort-enums.
        // Keep these values in sync with tools/wifi-abi-probe.c.
        assert!(core::mem::size_of::<VendorWpaApInfo>() == 52);
        assert!(core::mem::size_of::<VendorWpaAssoc>() == 111);
        assert!(core::mem::size_of::<VendorWpaEvent>() == 176);
    }
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
#[derive(Clone, Copy)]
enum ConnectionOutcome {
    Pending,
    Connected(ConnectionInfo),
    Failed(u16),
    Disconnected(u16),
}

#[cfg(target_arch = "riscv32")]
struct ConnectionState {
    active: Cell<bool>,
    done: Cell<bool>,
    outcome: Cell<ConnectionOutcome>,
}

#[cfg(target_arch = "riscv32")]
static CONNECTION_STATE: Mutex<ConnectionState> = Mutex::new(ConnectionState {
    active: Cell::new(false),
    done: Cell::new(false),
    outcome: Cell::new(ConnectionOutcome::Pending),
});

#[cfg(target_arch = "riscv32")]
fn finish_scan() {
    critical_section::with(|cs| SCAN_STATE.borrow(cs).active.set(false));
}

#[cfg(target_arch = "riscv32")]
fn finish_connection() {
    critical_section::with(|cs| CONNECTION_STATE.borrow(cs).active.set(false));
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
            result.security = if vendor.capabilities & 0x0010 == 0 {
                ScanSecurity::Open
            } else {
                ScanSecurity::Protected
            };
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
    } else if event == EVENT_CONNECT_RESULT
        && !data.is_null()
        && length as usize == core::mem::size_of::<VendorConnectResult>()
    {
        // SAFETY: the vendor callback reports the exact connect-result layout
        // and the value is copied before the callback returns.
        let result = unsafe { &*data.cast::<VendorConnectResult>() };
        let outcome = if result.status == 0 {
            ConnectionOutcome::Connected(ConnectionInfo {
                bssid: result.bssid,
                frequency_mhz: result.frequency_mhz,
            })
        } else {
            ConnectionOutcome::Failed(result.status)
        };
        critical_section::with(|cs| {
            let state = CONNECTION_STATE.borrow(cs);
            if state.active.get() {
                state.outcome.set(outcome);
                state.done.set(true);
            }
        });
    } else if event == EVENT_DISCONNECT
        && !data.is_null()
        && length as usize == core::mem::size_of::<VendorDisconnect>()
    {
        // SAFETY: the callback length was checked against the vendor layout.
        let disconnect = unsafe { &*data.cast::<VendorDisconnect>() };
        critical_section::with(|cs| {
            let state = CONNECTION_STATE.borrow(cs);
            if state.active.get() {
                state
                    .outcome
                    .set(ConnectionOutcome::Disconnected(disconnect.reason));
                state.done.set(true);
            }
        });
    }
    0
}

#[cfg(all(feature = "wifi-wpa2-personal", target_arch = "riscv32"))]
unsafe extern "C" fn wpa_event(event: *const VendorWpaEvent) {
    if event.is_null() {
        return;
    }
    // SAFETY: the registered callback receives a live ext_wifi_event for the
    // duration of this call. tools/wifi-abi-probe.c verifies info starts at 4.
    let event = unsafe { &*event };
    let outcome = match event.kind {
        2 => {
            let mut bssid = [0; 6];
            bssid.copy_from_slice(&event.info[33..39]);
            ConnectionOutcome::Connected(ConnectionInfo {
                bssid,
                frequency_mhz: 0,
            })
        }
        3 => ConnectionOutcome::Disconnected(u16::from_le_bytes([event.info[6], event.info[7]])),
        _ => return,
    };
    critical_section::with(|cs| {
        let state = CONNECTION_STATE.borrow(cs);
        if state.active.get() {
            state.outcome.set(outcome);
            state.done.set(true);
        }
    });
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
    fn uapi_ioctl_assoc(ifname: *const c_char, params: *mut c_void) -> c_int;
    #[cfg(feature = "wifi-wpa2-personal")]
    fn uapi_wifi_sta_start(ifname: *mut c_char, length: *mut c_int) -> c_int;
    #[cfg(feature = "wifi-wpa2-personal")]
    fn uapi_wifi_sta_scan() -> c_int;
    #[cfg(feature = "wifi-wpa2-personal")]
    fn uapi_wifi_get_scan_flag() -> c_int;
    #[cfg(feature = "wifi-wpa2-personal")]
    fn uapi_wifi_get_scan_results(results: *mut VendorWpaApInfo, count: *mut c_uint) -> c_int;
    #[cfg(feature = "wifi-wpa2-personal")]
    fn uapi_drv_cipher_env_init();
    #[cfg(feature = "wifi-wpa2-personal")]
    fn mbedtls_adapt_register_func() -> c_int;
    #[cfg(feature = "wifi-wpa2-personal")]
    fn uapi_wifi_sta_connect(request: *const VendorWpaAssoc) -> c_int;
    #[cfg(feature = "wifi-wpa2-personal")]
    fn uapi_wifi_config_callback(mode: u8, task_priority: u8, stack_size: u16) -> c_int;
    #[cfg(feature = "wifi-wpa2-personal")]
    fn uapi_wifi_register_event_callback(
        callback: Option<unsafe extern "C" fn(*const VendorWpaEvent)>,
    ) -> c_int;
}

#[cfg(test)]
mod tests {
    use super::{Error, OpenNetwork, ScanResult, ScanSecurity, ssid_from_ies};

    #[test]
    fn finds_ssid_information_element() {
        assert_eq!(ssid_from_ies(&[1, 1, 0x82, 0, 3, b'a', b'p', b'1']), b"ap1");
    }

    #[test]
    fn rejects_truncated_information_element() {
        assert_eq!(ssid_from_ies(&[0, 4, b'a', b'b']), b"");
    }

    #[test]
    fn open_network_is_constructed_from_a_scan_result() {
        let mut result = ScanResult::empty();
        result.ssid[..4].copy_from_slice(b"open");
        result.ssid_len = 4;
        result.bssid = [1, 2, 3, 4, 5, 6];
        result.frequency_mhz = 2437;

        let network = OpenNetwork::from_scan(&result).unwrap();
        assert_eq!(network.ssid(), b"open");
        assert_eq!(network.bssid, result.bssid);
        assert_eq!(network.frequency_mhz, 2437);
    }

    #[test]
    fn hidden_scan_result_is_not_a_connectable_open_network() {
        assert_eq!(
            OpenNetwork::from_scan(&ScanResult::empty()),
            Err(Error::InvalidSsid)
        );
    }

    #[test]
    fn protected_scan_result_is_not_a_connectable_open_network() {
        let mut result = ScanResult::empty();
        result.ssid[0] = b'x';
        result.ssid_len = 1;
        result.security = ScanSecurity::Protected;
        assert_eq!(
            OpenNetwork::from_scan(&result),
            Err(Error::ProtectedNetwork)
        );
    }
}
