//! `errcode_t` mapping for the public Wi-Fi API (future use).
//!
//! The vendor public API (`wifi_init`, `wifi_sta_scan`, `wifi_sta_connect`, …,
//! declared in `ws63-radio-sys/ws63-RF/include/api/wifi/`) returns `errcode_t` (0 = success).
//! NOTE: the current blob delivery exports the lower-level `uapi_wifi_init`
//! symbol from `libwifi_driver_hmac.a`, while the public header still declares
//! `wifi_init`. The guarded two-pass RF build can now produce the full init
//! image; a safe Rust API remains deferred until the on-silicon init contract
//! and its error classes are known. This module provides the error mapping that
//! binding will use.

/// Vendor `errcode_t` (0 = success).
pub type Errcode = u32;

/// `ERRCODE_SUCC`.
pub const ERRCODE_SUCC: Errcode = 0;

/// A non-zero vendor error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WifiError(pub Errcode);

/// Map an `errcode_t` to a `Result`.
pub fn check(code: Errcode) -> Result<(), WifiError> {
    if code == ERRCODE_SUCC {
        Ok(())
    } else {
        Err(WifiError(code))
    }
}
