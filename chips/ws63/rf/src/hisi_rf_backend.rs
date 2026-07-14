//! WS63 implementation of the chip-neutral `hisi-rf` control contract.

use hisi_hal::peripherals::Efuse;
use hisi_rf::{
    BackendError, BackendErrorClass, ConnectionInfo, RadioConfig, RadioState, ScanConfig,
    ScanOutcome, ScanResult, Security, Ssid, StationConfig, WifiBackend,
};

#[cfg(feature = "net")]
use crate::netif_smoltcp::Ws63Device;
#[cfg(not(feature = "upstream-supplicant-port"))]
use crate::wifi::{ConnectionInfo as Ws63ConnectionInfo, PersonalNetwork, WpaWifi as ActiveWifi};
use crate::wifi::{
    Error as Ws63Error, MAX_SCAN_RESULTS, ScanResult as Ws63ScanResult, ScanSecurity,
};
#[cfg(feature = "upstream-supplicant-port")]
use crate::{
    upstream_supplicant::{NativeSupplicant, NativeSupplicantError},
    wifi::Wifi as ActiveWifi,
};
#[cfg(feature = "net")]
use hisi_rf::RadioResources;

/// WS63 control-plane resources before the vendor runtime is initialized.
pub struct Ws63WifiBackend<'d> {
    efuse: Option<Efuse<'d>>,
    wifi: Option<ActiveWifi<'d>>,
    #[cfg(feature = "upstream-supplicant-port")]
    supplicant: Option<NativeSupplicant>,
    scans: [Ws63ScanResult; MAX_SCAN_RESULTS],
    scan_count: usize,
}

impl<'d> Ws63WifiBackend<'d> {
    /// Bind the one-shot eFuse token needed by the WS63 vendor runtime.
    pub fn new(efuse: Efuse<'d>) -> Self {
        Self {
            efuse: Some(efuse),
            wifi: None,
            #[cfg(feature = "upstream-supplicant-port")]
            supplicant: None,
            scans: [Ws63ScanResult::empty(); MAX_SCAN_RESULTS],
            scan_count: 0,
        }
    }
}

impl WifiBackend for Ws63WifiBackend<'_> {
    fn initialize(&mut self, _: &hisi_rf::WifiConfig) -> Result<(), BackendError> {
        if self.wifi.is_some() {
            return Ok(());
        }
        let efuse = self.efuse.take().ok_or(BackendError {
            class: BackendErrorClass::Initialize,
            code: 0x1000_0001,
        })?;
        let wifi = ActiveWifi::initialize(efuse).map_err(map_error)?;
        #[cfg(feature = "upstream-supplicant-port")]
        {
            self.supplicant =
                Some(NativeSupplicant::create(wifi.interface_name()).map_err(map_native_error)?);
        }
        self.wifi = Some(wifi);
        Ok(())
    }

    fn scan(
        &mut self,
        config: ScanConfig,
        output: &mut [ScanResult],
    ) -> Result<ScanOutcome, BackendError> {
        let wifi = self.wifi.as_mut().ok_or(not_initialized())?;
        self.scan_count = wifi
            .scan(&mut self.scans, config.timeout_ms())
            .map_err(map_error)?;

        let mut written = 0;
        for scan in &self.scans[..self.scan_count] {
            let Some(ssid) = Ssid::try_from_bytes(scan.ssid()) else {
                continue;
            };
            if written == output.len() {
                break;
            }
            output[written] = ScanResult {
                ssid,
                bssid: scan.bssid,
                frequency_mhz: scan.frequency_mhz,
                rssi_dbm: scan.rssi_dbm,
                security: match scan.security() {
                    ScanSecurity::Open => Security::Open,
                    #[cfg(feature = "wifi-wpa3-personal")]
                    ScanSecurity::Protected if scan.supports_wpa3_personal() => {
                        Security::Wpa3Personal
                    }
                    ScanSecurity::Protected if scan.supports_wpa2_personal() => {
                        Security::Wpa2Personal
                    }
                    ScanSecurity::Protected => Security::OtherProtected,
                },
                channel: scan.channel(),
            };
            written += 1;
        }
        Ok(ScanOutcome {
            count: written,
            truncated: written < self.scan_count,
        })
    }

    fn connect(&mut self, config: &StationConfig) -> Result<ConnectionInfo, BackendError> {
        #[cfg(feature = "upstream-supplicant-port")]
        {
            let _ = config;
            Err(BackendError {
                class: BackendErrorClass::UnsupportedSecurity,
                code: 0x5732_0001,
            })
        }
        #[cfg(not(feature = "upstream-supplicant-port"))]
        {
            let wifi = self.wifi.as_mut().ok_or(not_initialized())?;
            let scan = self.scans[..self.scan_count]
                .iter()
                .find(|scan| {
                    scan.ssid() == config.ssid.as_bytes()
                        && scan.bssid == config.bssid
                        && scan.channel() == config.channel
                })
                .ok_or(BackendError {
                    class: BackendErrorClass::Connect,
                    code: 0x1000_0002,
                })?;
            let network = PersonalNetwork::from_scan_with_security(
                scan,
                config.passphrase.expose_secret(),
                config.security(),
            )
            .map_err(map_error)?;
            wifi.connect(&network, config.timeout_ms())
                .map(to_connection_info)
                .map_err(map_error)
        }
    }

    fn disconnect(&mut self, config: &hisi_rf::WifiConfig) -> Result<(), BackendError> {
        #[cfg(feature = "upstream-supplicant-port")]
        {
            let _ = config;
            Err(BackendError {
                class: BackendErrorClass::UnsupportedSecurity,
                code: 0x5732_0002,
            })
        }
        #[cfg(not(feature = "upstream-supplicant-port"))]
        {
            self.wifi
                .as_mut()
                .ok_or(not_initialized())?
                .disconnect(config.disconnect_timeout_ms)
                .map_err(map_error)
        }
    }

    fn poll(&mut self) -> Result<bool, BackendError> {
        #[cfg(feature = "upstream-supplicant-port")]
        {
            let Some(supplicant) = self.supplicant.as_mut() else {
                return Ok(false);
            };
            let result = supplicant
                .poll(core::num::NonZeroU32::new(32).unwrap())
                .map_err(map_native_error)?;
            Ok(result.work_pending != 0)
        }
        #[cfg(not(feature = "upstream-supplicant-port"))]
        {
            Ok(false)
        }
    }
}

/// Build the WS63 resources consumed by `hisi_rf::init`.
#[cfg(feature = "net")]
pub fn resources(efuse: Efuse<'static>) -> RadioResources<Ws63WifiBackend<'static>, Ws63Device> {
    RadioResources {
        backend: Ws63WifiBackend::new(efuse),
        device: Ws63Device,
    }
}

/// Convenience type for the WS63 Wi-Fi-only controller state.
pub type Ws63RadioState<const EVENTS: usize> = RadioState<EVENTS>;

/// Default radio configuration for the WS63 Personal-mode station path.
pub fn config() -> RadioConfig {
    RadioConfig::default()
}

#[cfg(not(feature = "upstream-supplicant-port"))]
fn to_connection_info(info: Ws63ConnectionInfo) -> ConnectionInfo {
    ConnectionInfo {
        bssid: info.bssid,
        frequency_mhz: info.frequency_mhz,
    }
}

fn not_initialized() -> BackendError {
    BackendError {
        class: BackendErrorClass::Initialize,
        code: 0x1000_0003,
    }
}

fn map_error(error: Ws63Error) -> BackendError {
    let (class, code) = match error {
        Ws63Error::Initialize(code) => (BackendErrorClass::Initialize, code),
        Ws63Error::Busy => (BackendErrorClass::Busy, 1),
        Ws63Error::Timeout => (BackendErrorClass::Timeout, 1),
        Ws63Error::UnsupportedSecurity(mode) => {
            (BackendErrorClass::UnsupportedSecurity, mode as u32)
        }
        Ws63Error::ConnectFailed(status) => (BackendErrorClass::Connect, status as u32),
        Ws63Error::Disconnected(reason) => (BackendErrorClass::Connect, reason as u32),
        Ws63Error::ConfigureSecurity(code)
        | Ws63Error::StartConnect(code)
        | Ws63Error::StartDisconnect(code) => (BackendErrorClass::Connect, code as u32),
        Ws63Error::Runtime(code) => (BackendErrorClass::Other, 0x2000_0000 | runtime_code(code)),
        Ws63Error::AlreadyInitialized => (BackendErrorClass::Initialize, 2),
        #[cfg(feature = "upstream-supplicant-port")]
        Ws63Error::SupplicantPort(_) => (BackendErrorClass::Initialize, 3),
        Ws63Error::CreateStation(code)
        | Ws63Error::RegisterEvents(code)
        | Ws63Error::OpenStation(code)
        | Ws63Error::StartScan(code) => (BackendErrorClass::Other, code as u32),
        Ws63Error::Timebase(code) | Ws63Error::Crypto(code) => (BackendErrorClass::Other, code),
        Ws63Error::ScanFailed(status) => (BackendErrorClass::Other, scan_status_code(status)),
        Ws63Error::InvalidSsid => (BackendErrorClass::Other, 0x100),
        Ws63Error::ProtectedNetwork | Ws63Error::OpenNetwork | Ws63Error::InvalidPassphrase => {
            (BackendErrorClass::UnsupportedSecurity, 0x101)
        }
        Ws63Error::UnsupportedTarget => (BackendErrorClass::Other, u32::MAX),
    };
    BackendError { class, code }
}

#[cfg(feature = "upstream-supplicant-port")]
fn map_native_error(error: NativeSupplicantError) -> BackendError {
    let code = match error {
        NativeSupplicantError::Port(_) => 1,
        NativeSupplicantError::InvalidContextLayout => 2,
        NativeSupplicantError::AllocationFailed => 3,
        NativeSupplicantError::CreateFailed => 4,
        NativeSupplicantError::InitializeFailed(status) => 0x1000 | status as u32 & 0xfff,
        NativeSupplicantError::EnableEapolFailed(status) => 0x2000 | status as u32 & 0xfff,
        NativeSupplicantError::FeedMgmtFailed(status) => 0x3000 | status as u32 & 0xfff,
        NativeSupplicantError::FeedEapolFailed(status) => 0x4000 | status as u32 & 0xfff,
        NativeSupplicantError::MgmtQueueOverflow(count) => 0x6000 | count.min(0xfff),
        NativeSupplicantError::InvalidResult => 5,
        NativeSupplicantError::PollFailed(status) => 0x5000 | status as u32 & 0xfff,
    };
    BackendError {
        class: BackendErrorClass::Initialize,
        code: 0x5732_0000 | code,
    }
}

fn runtime_code(error: hisi_rf_rtos_driver::Error) -> u32 {
    use hisi_rf_rtos_driver::Error;
    match error {
        Error::NotInstalled => 1,
        Error::AlreadyInstalled => 2,
        Error::ResourceExhausted => 3,
        Error::NoTaskSlots => 4,
        Error::InvalidHandle => 5,
        Error::InvalidContext => 6,
        Error::TimedOut => 7,
        Error::Runtime => 8,
    }
}

fn scan_status_code(status: crate::wifi::ScanStatus) -> u32 {
    use crate::wifi::ScanStatus;
    match status {
        ScanStatus::Success => 0,
        ScanStatus::Failed => 1,
        ScanStatus::Refused => 2,
        ScanStatus::Timeout => 3,
        ScanStatus::Unknown(code) => code,
    }
}
