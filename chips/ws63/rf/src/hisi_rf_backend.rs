//! WS63 implementation of the chip-neutral `hisi-rf` control contract.

use hisi_hal::peripherals::{Efuse, Km, Pke, Spacc, Trng};
use hisi_rf::{
    BackendError, BackendErrorClass, ConnectionInfo, RadioConfig, RadioState, ScanConfig,
    ScanOutcome, ScanResult, Security, Ssid, StationConfig, WifiBackend,
};

#[cfg(feature = "upstream-supplicant-port")]
const NATIVE_EVENT_AUTHORIZED: u8 = 3;
#[cfg(feature = "upstream-supplicant-port")]
const NATIVE_EVENT_DISCONNECTED: u8 = 4;
#[cfg(feature = "upstream-supplicant-port")]
const NATIVE_EVENT_FAILED: u8 = 5;

#[cfg(feature = "upstream-supplicant-port")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeConnectEvent {
    Progress,
    Authorized,
    Disconnected,
    Failed,
}

#[cfg(feature = "upstream-supplicant-port")]
const fn classify_native_connect_event(kind: u8) -> NativeConnectEvent {
    match kind {
        NATIVE_EVENT_AUTHORIZED => NativeConnectEvent::Authorized,
        NATIVE_EVENT_DISCONNECTED => NativeConnectEvent::Disconnected,
        NATIVE_EVENT_FAILED => NativeConnectEvent::Failed,
        _ => NativeConnectEvent::Progress,
    }
}

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
    km: Option<Km<'d>>,
    spacc: Option<Spacc<'d>>,
    pke: Option<Pke<'d>>,
    trng: Option<Trng<'d>>,
    wifi: Option<ActiveWifi<'d>>,
    #[cfg(feature = "upstream-supplicant-port")]
    supplicant: Option<NativeSupplicant>,
    scans: [Ws63ScanResult; MAX_SCAN_RESULTS],
    scan_count: usize,
}

impl<'d> Ws63WifiBackend<'d> {
    /// Bind the one-shot eFuse token needed by the WS63 vendor runtime.
    pub fn new(
        efuse: Efuse<'d>,
        km: Km<'d>,
        spacc: Spacc<'d>,
        pke: Pke<'d>,
        trng: Trng<'d>,
    ) -> Self {
        Self {
            efuse: Some(efuse),
            km: Some(km),
            spacc: Some(spacc),
            pke: Some(pke),
            trng: Some(trng),
            wifi: None,
            #[cfg(feature = "upstream-supplicant-port")]
            supplicant: None,
            scans: [Ws63ScanResult::empty(); MAX_SCAN_RESULTS],
            scan_count: 0,
        }
    }
}

impl WifiBackend for Ws63WifiBackend<'static> {
    fn initialize(&mut self, _: &hisi_rf::WifiConfig) -> Result<(), BackendError> {
        if self.wifi.is_some() {
            return Ok(());
        }
        let efuse = self.efuse.take().ok_or(BackendError::new(
            BackendErrorClass::Initialize,
            0x1000_0001,
        ))?;
        let trng = self.trng.take().ok_or(BackendError::new(
            BackendErrorClass::Initialize,
            0x1000_0004,
        ))?;
        let km = self.km.take().ok_or(BackendError::new(
            BackendErrorClass::Initialize,
            0x1000_0005,
        ))?;
        let spacc = self.spacc.take().ok_or(BackendError::new(
            BackendErrorClass::Initialize,
            0x1000_0006,
        ))?;
        let pke = self.pke.take().ok_or(BackendError::new(
            BackendErrorClass::Initialize,
            0x1000_0007,
        ))?;
        crate::crypto::install_hardware_crypto(km, spacc, pke, trng)
            .map_err(|error| BackendError::new(BackendErrorClass::Initialize, error.code()))?;
        #[cfg(target_arch = "riscv32")]
        crate::crypto::ws63_pbkdf2_self_test()
            .map_err(|error| BackendError::new(BackendErrorClass::Initialize, error.code()))?;
        #[cfg(target_arch = "riscv32")]
        crate::crypto::ws63_hash_self_test()
            .map_err(|error| BackendError::new(BackendErrorClass::Initialize, error.code()))?;
        #[cfg(all(target_arch = "riscv32", feature = "upstream-supplicant-wpa3"))]
        crate::crypto::ws63_p256_self_test()
            .map_err(|error| BackendError::new(BackendErrorClass::Initialize, error.code()))?;
        #[cfg(all(target_arch = "riscv32", feature = "rf-eloop-diag"))]
        crate::crypto::ws63_hash_fault_recovery_self_test()
            .map_err(|error| BackendError::new(BackendErrorClass::Initialize, error.code()))?;
        #[cfg(all(target_arch = "riscv32", feature = "rf-eloop-diag"))]
        crate::crypto::ws63_cipher_fault_recovery_self_test()
            .map_err(|error| BackendError::new(BackendErrorClass::Initialize, error.code()))?;
        #[cfg(all(target_arch = "riscv32", feature = "rf-crypto-contention-diag"))]
        crate::crypto::ws63_crypto_contention_self_test()
            .map_err(|error| BackendError::new(BackendErrorClass::Initialize, error.code()))?;
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
        #[cfg(feature = "upstream-supplicant-port")]
        self.supplicant
            .as_mut()
            .ok_or(not_initialized())?
            .begin_scan_cache_capture()
            .map_err(map_native_error)?;

        let scan = match self.wifi.as_mut() {
            Some(wifi) => wifi.scan(&mut self.scans, config.timeout_ms()),
            None => {
                #[cfg(feature = "upstream-supplicant-port")]
                if let Some(supplicant) = self.supplicant.as_mut() {
                    supplicant.cancel_scan_cache_capture();
                }
                return Err(not_initialized());
            }
        };
        self.scan_count = match scan {
            Ok(count) => count,
            Err(error) => {
                #[cfg(feature = "upstream-supplicant-port")]
                if let Some(supplicant) = self.supplicant.as_mut() {
                    supplicant.cancel_scan_cache_capture();
                }
                return Err(map_error(error));
            }
        };

        #[cfg(feature = "upstream-supplicant-port")]
        match self.supplicant.as_mut() {
            Some(supplicant) => supplicant
                .finish_scan_cache_capture()
                .map_err(map_native_error)?,
            None => return Err(not_initialized()),
        }

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
                    #[cfg(any(
                        feature = "wifi-wpa3-personal",
                        feature = "upstream-supplicant-wpa3"
                    ))]
                    ScanSecurity::Protected if scan.supports_wpa2_wpa3_transition() => {
                        Security::Wpa2Wpa3PersonalTransition
                    }
                    #[cfg(any(
                        feature = "wifi-wpa3-personal",
                        feature = "upstream-supplicant-wpa3"
                    ))]
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
            let supplicant = self.supplicant.as_mut().ok_or(not_initialized())?;
            supplicant.configure(config).map_err(map_native_error)?;
            supplicant.connect().map_err(map_native_error)?;
            let started_at = crate::uapi::monotonic_ms();
            let mut last_event_kind = 0_u8;
            let mut last_disconnect_status = None;
            loop {
                supplicant
                    .poll(core::num::NonZeroU32::new(32).unwrap())
                    .map_err(map_native_error)?;
                while let Some(event) = supplicant.next_event().map_err(map_native_error)? {
                    last_event_kind = event.kind;
                    match classify_native_connect_event(event.kind) {
                        NativeConnectEvent::Progress => {}
                        NativeConnectEvent::Authorized => {
                            return Ok(ConnectionInfo {
                                bssid: config.bssid,
                                frequency_mhz: channel_to_frequency(config.channel),
                            });
                        }
                        NativeConnectEvent::Disconnected => {
                            // DISCONNECTED is an intermediate supplicant state,
                            // not a terminal connect result. In particular,
                            // hostap retries association after temporary PMF
                            // rejection and other recoverable driver failures.
                            // Keep driving its event loop until AUTHORIZED or
                            // the caller's overall connect deadline expires.
                            if event.status != 0 {
                                last_disconnect_status = Some(event.status);
                            }
                        }
                        NativeConnectEvent::Failed => {
                            let code = emit_backend_failure(supplicant, event.status);
                            return Err(BackendError::new(BackendErrorClass::Connect, code));
                        }
                    }
                }
                if crate::uapi::monotonic_ms().wrapping_sub(started_at)
                    >= config.timeout_ms() as u64
                {
                    if let Some(status) = last_disconnect_status {
                        let code = emit_backend_failure(supplicant, status);
                        let _ = supplicant.disconnect();
                        return Err(BackendError::new(BackendErrorClass::Connect, code));
                    }
                    let context_diagnostic = supplicant.context_diagnostic_word();
                    let port_diagnostic = crate::upstream_supplicant::diagnostic_word();
                    let _ = supplicant.disconnect();
                    return Err(BackendError::new(
                        BackendErrorClass::Timeout,
                        0x8000_0000
                            | ((last_event_kind as u32 & 0x7) << 28)
                            | ((context_diagnostic & 0x0fff) << 16)
                            | port_diagnostic,
                    ));
                }
                hisi_rf_rtos_driver::sleep_ms(core::num::NonZeroU32::new(1).unwrap()).map_err(
                    |error| {
                        BackendError::new(
                            BackendErrorClass::Other,
                            0x5732_e000 | runtime_code(error),
                        )
                    },
                )?;
            }
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
                .ok_or(BackendError::new(BackendErrorClass::Connect, 0x1000_0002))?;
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
            let supplicant = self.supplicant.as_mut().ok_or(not_initialized())?;
            supplicant.disconnect().map_err(map_native_error)?;
            let started_at = crate::uapi::monotonic_ms();
            loop {
                supplicant
                    .poll(core::num::NonZeroU32::new(32).unwrap())
                    .map_err(map_native_error)?;
                while let Some(event) = supplicant.next_event().map_err(map_native_error)? {
                    match event.kind {
                        NATIVE_EVENT_DISCONNECTED => return Ok(()),
                        NATIVE_EVENT_FAILED => {
                            return Err(BackendError::new(
                                BackendErrorClass::Connect,
                                event.status as u32,
                            ));
                        }
                        _ => {}
                    }
                }
                if crate::uapi::monotonic_ms().wrapping_sub(started_at)
                    >= config.disconnect_timeout_ms as u64
                {
                    return Err(BackendError::new(BackendErrorClass::Timeout, 2));
                }
                hisi_rf_rtos_driver::sleep_ms(core::num::NonZeroU32::new(1).unwrap()).map_err(
                    |error| {
                        BackendError::new(
                            BackendErrorClass::Other,
                            0x5732_e000 | runtime_code(error),
                        )
                    },
                )?;
            }
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

#[cfg(feature = "upstream-supplicant-port")]
fn emit_backend_failure(supplicant: &NativeSupplicant, status: i32) -> u32 {
    let context_diagnostic = supplicant.context_diagnostic_word();
    let port_diagnostic = crate::upstream_supplicant::diagnostic_word();
    crate::log_emit(b"RFDBG_WPA_BACKEND_ERR status=");
    crate::upstream_supplicant::emit_diagnostic_hex(status as u32);
    crate::log_emit(b" context=");
    crate::upstream_supplicant::emit_diagnostic_hex(context_diagnostic);
    crate::log_emit(b" port=");
    crate::upstream_supplicant::emit_diagnostic_hex(port_diagnostic);
    crate::log_emit(b"\r\n");
    // The UART sink is best-effort while the radio runner and vendor workers
    // are active. Preserve the same snapshot in the public error code so one
    // terminal marker is sufficient to diagnose an early hostap rejection.
    // The raw terminal status remains available in the log above.
    0xd000_0000 | ((context_diagnostic & 0x0fff) << 16) | (port_diagnostic & 0xffff)
}

/// Build the WS63 resources consumed by `hisi_rf::init`.
#[cfg(feature = "net")]
pub fn resources(
    efuse: Efuse<'static>,
    km: Km<'static>,
    spacc: Spacc<'static>,
    pke: Pke<'static>,
    trng: Trng<'static>,
) -> RadioResources<Ws63WifiBackend<'static>, Ws63Device> {
    RadioResources {
        backend: Ws63WifiBackend::new(efuse, km, spacc, pke, trng),
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
    BackendError::new(BackendErrorClass::Initialize, 0x1000_0003)
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
        Ws63Error::CryptoInitialize(code) => (BackendErrorClass::Initialize, code as u32),
        Ws63Error::Timebase(code) | Ws63Error::Crypto(code) => (BackendErrorClass::Other, code),
        Ws63Error::ScanFailed(status) => (BackendErrorClass::Other, scan_status_code(status)),
        Ws63Error::InvalidSsid => (BackendErrorClass::Other, 0x100),
        Ws63Error::ProtectedNetwork | Ws63Error::OpenNetwork | Ws63Error::InvalidPassphrase => {
            (BackendErrorClass::UnsupportedSecurity, 0x101)
        }
        Ws63Error::UnsupportedTarget => (BackendErrorClass::Other, u32::MAX),
    };
    BackendError::new(class, code)
}

#[cfg(feature = "upstream-supplicant-port")]
fn map_native_error(error: NativeSupplicantError) -> BackendError {
    let (class, code) = match error {
        NativeSupplicantError::Port(_) => (BackendErrorClass::Initialize, 1),
        NativeSupplicantError::InvalidContextLayout => (BackendErrorClass::Initialize, 2),
        NativeSupplicantError::AllocationFailed => (BackendErrorClass::Initialize, 3),
        NativeSupplicantError::CreateFailed => (BackendErrorClass::Initialize, 4),
        NativeSupplicantError::InitializeFailed(status) => (
            BackendErrorClass::Initialize,
            0x1000 | status as u32 & 0xfff,
        ),
        NativeSupplicantError::EnableEapolFailed(status) => (
            BackendErrorClass::Initialize,
            0x2000 | status as u32 & 0xfff,
        ),
        NativeSupplicantError::FeedMgmtFailed(status) => {
            (BackendErrorClass::Other, 0x3000 | status as u32 & 0xfff)
        }
        NativeSupplicantError::FeedEapolFailed(status) => {
            (BackendErrorClass::Other, 0x4000 | status as u32 & 0xfff)
        }
        NativeSupplicantError::MgmtQueueOverflow(count) => {
            (BackendErrorClass::Other, 0x6000 | count.min(0xfff))
        }
        NativeSupplicantError::FeedScanFailed(status) => {
            (BackendErrorClass::Other, 0x7000 | status as u32 & 0xfff)
        }
        NativeSupplicantError::ScanQueueOverflow(count) => {
            (BackendErrorClass::Other, 0x8000 | count.min(0xfff))
        }
        NativeSupplicantError::FeedLinkFailed(status) => {
            (BackendErrorClass::Connect, 0x9000 | status as u32 & 0xfff)
        }
        NativeSupplicantError::LinkQueueOverflow(count) => {
            (BackendErrorClass::Connect, 0xa000 | count.min(0xfff))
        }
        NativeSupplicantError::FeedExternalAuthFailed(status) => {
            (BackendErrorClass::Connect, 0xe000 | status as u32 & 0xfff)
        }
        NativeSupplicantError::ExternalAuthQueueOverflow(count) => {
            (BackendErrorClass::Connect, 0xf000 | count.min(0xfff))
        }
        NativeSupplicantError::ConfigureFailed(status) => {
            (BackendErrorClass::Connect, 0xb000 | status as u32 & 0xfff)
        }
        NativeSupplicantError::ConnectFailed(status) => {
            (BackendErrorClass::Connect, 0xc000 | status as u32 & 0xfff)
        }
        NativeSupplicantError::DisconnectFailed(status) => {
            (BackendErrorClass::Connect, 0xd000 | status as u32 & 0xfff)
        }
        NativeSupplicantError::InvalidResult => (BackendErrorClass::Other, 5),
        NativeSupplicantError::PollFailed(status) => {
            (BackendErrorClass::Other, 0x5000 | status as u32 & 0xfff)
        }
    };
    BackendError::new(class, 0x5732_0000 | code)
}

#[cfg(feature = "upstream-supplicant-port")]
const fn channel_to_frequency(channel: u8) -> u16 {
    if channel == 14 {
        2484
    } else if channel >= 1 && channel <= 13 {
        2407 + channel as u16 * 5
    } else {
        0
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
        Error::IncompatibleContract => 9,
        Error::IncompatibleExecutionProfile => 10,
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

#[cfg(all(test, feature = "upstream-supplicant-port"))]
mod tests {
    use super::{
        NATIVE_EVENT_AUTHORIZED, NATIVE_EVENT_DISCONNECTED, NATIVE_EVENT_FAILED,
        NativeConnectEvent, classify_native_connect_event,
    };

    #[test]
    fn disconnected_is_recoverable_until_the_overall_connect_deadline() {
        assert_eq!(
            classify_native_connect_event(NATIVE_EVENT_DISCONNECTED),
            NativeConnectEvent::Disconnected
        );
        assert_eq!(
            classify_native_connect_event(NATIVE_EVENT_FAILED),
            NativeConnectEvent::Failed
        );
        assert_eq!(
            classify_native_connect_event(NATIVE_EVENT_AUTHORIZED),
            NativeConnectEvent::Authorized
        );
        assert_eq!(
            classify_native_connect_event(1),
            NativeConnectEvent::Progress
        );
        assert_eq!(
            classify_native_connect_event(2),
            NativeConnectEvent::Progress
        );
    }
}
