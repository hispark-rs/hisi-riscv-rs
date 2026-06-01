//! `errcode_t` mapping for the public Wi-Fi API (future use).
//!
//! The vendor public API (`wifi_init`, `wifi_sta_scan`, `wifi_sta_connect`, …,
//! declared in `ws63-RF/include/api/wifi/`) returns `errcode_t` (0 = success).
//! NOTE: those API symbols are **not** exported by any blob shipped in
//! `ws63-RF/lib` — they live in the host-MAC library (`libwifi_driver_hmac.a`),
//! which is not part of this delivery. So a safe Rust API over them is deferred
//! until that layer is available; this module only provides the error mapping
//! the future binding will use.

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
