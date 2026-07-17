//! Internal Wi-Fi security provider boundary.
//!
//! The C supplicant must not make mbedTLS contexts part of the Rust API. WS63
//! uses token-owned WS63 RKP/SPACC capabilities where proven on silicon and
//! SPACC AES primitives where proven on silicon. `hisi-crypto` owns the
//! capability contract; RF owns only this WS63 service and C ABI shim.

pub(crate) use hisi_crypto::CryptoError;
#[cfg(all(
    target_arch = "riscv32",
    any(feature = "wifi-wpa2-personal", feature = "upstream-supplicant-port")
))]
use hisi_crypto::Pbkdf2HmacSha1;
#[cfg(target_arch = "riscv32")]
use hisi_crypto::{
    EntropySource, TryBlockCipher, TryHash, TryMac,
    sae::{
        P256AffinePoint, P256FieldElement, P256PointResult, TryP256ComputeYSquared,
        TryP256FieldMul, TryP256FieldPow, TryP256PointAdd, TryP256PointInvert, TryP256PointMul,
        TryP256PointValidate,
    },
};
#[cfg(target_arch = "riscv32")]
use hisi_crypto_ws63::{Ws63Crypto, Ws63CryptoResources, Ws63CryptoStorage, Ws63P256};
use hisi_hal::peripherals::{Km, Pke, Spacc, Trng};
#[cfg(target_arch = "riscv32")]
use hisi_rf_rtos_driver::{MutexHandle, WaitOutcome, WaitTimeout};
#[cfg(target_arch = "riscv32")]
use portable_atomic::{AtomicPtr, AtomicU32, Ordering};
#[cfg(target_arch = "riscv32")]
use static_cell::StaticCell;
#[cfg(target_arch = "riscv32")]
use zeroize::Zeroize;

#[cfg(feature = "upstream-supplicant-wpa3")]
#[path = "crypto_sae.rs"]
mod crypto_sae;

#[cfg(target_arch = "riscv32")]
struct CryptoService {
    backend: Ws63Crypto<'static>,
    p256: Ws63P256<'static>,
    mutex: MutexHandle,
}

#[cfg(target_arch = "riscv32")]
static CRYPTO_CELL: StaticCell<CryptoService> = StaticCell::new();
#[cfg(target_arch = "riscv32")]
static CRYPTO_STORAGE: StaticCell<Ws63CryptoStorage> = StaticCell::new();
#[cfg(target_arch = "riscv32")]
static CRYPTO_SERVICE: AtomicPtr<CryptoService> = AtomicPtr::new(core::ptr::null_mut());
#[cfg(target_arch = "riscv32")]
static ENTROPY_REQUESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static ENTROPY_BYTES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static ENTROPY_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static PBKDF2_REQUESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static PBKDF2_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static PBKDF2_TOTAL_MS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static PBKDF2_MAX_MS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static HASH_REQUESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static HASH_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static HASH_TOTAL_MS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static HASH_MAX_MS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static MAC_REQUESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static MAC_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static MAC_TOTAL_MS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static MAC_MAX_MS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static HASH_RECOVERY_TESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static HASH_RECOVERY_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static CIPHER_REQUESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static CIPHER_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static CIPHER_TOTAL_MS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static CIPHER_MAX_MS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static CIPHER_RECOVERY_TESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static CIPHER_RECOVERY_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_REQUESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_TOTAL_MS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_MAX_MS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_ADD_REQUESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_ADD_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_ADD_TOTAL_MS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_ADD_MAX_MS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_FIELD_REQUESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_FIELD_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_FIELD_TOTAL_MS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_FIELD_MAX_MS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_FIELD_MUL_REQUESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_FIELD_SQUARE_REQUESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_FIELD_POW_REQUESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_FIELD_MUL_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_FIELD_SQUARE_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_FIELD_POW_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_CURVE_REQUESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_CURVE_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_CURVE_TOTAL_MS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_CURVE_MAX_MS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_CURVE_INVERT_REQUESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_CURVE_VALIDATE_REQUESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_CURVE_Y2_REQUESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_CURVE_INVERT_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_CURVE_VALIDATE_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "riscv32")]
static P256_CURVE_Y2_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(all(target_arch = "riscv32", feature = "rf-crypto-contention-diag"))]
static CRYPTO_CONTENTION_CONTEXT: StaticCell<CryptoContentionContext> = StaticCell::new();
#[cfg(all(target_arch = "riscv32", feature = "rf-crypto-contention-diag"))]
static CRYPTO_CONTENTION_TESTS: AtomicU32 = AtomicU32::new(0);
#[cfg(all(target_arch = "riscv32", feature = "rf-crypto-contention-diag"))]
static CRYPTO_CONTENTION_FAILURES: AtomicU32 = AtomicU32::new(0);
#[cfg(all(target_arch = "riscv32", feature = "rf-crypto-contention-diag"))]
static CRYPTO_CONTENTION_OBSERVED: AtomicU32 = AtomicU32::new(0);
#[cfg(all(target_arch = "riscv32", feature = "rf-crypto-contention-diag"))]
static CRYPTO_CONTENTION_HOLDER_COMPLETIONS: AtomicU32 = AtomicU32::new(0);
#[cfg(all(target_arch = "riscv32", feature = "rf-crypto-contention-diag"))]
static CRYPTO_CONTENTION_WAITER_COMPLETIONS: AtomicU32 = AtomicU32::new(0);

#[cfg(all(target_arch = "riscv32", feature = "rf-crypto-contention-diag"))]
struct CryptoContentionContext {
    start: hisi_rf_rtos_driver::SemaphoreHandle,
    done: hisi_rf_rtos_driver::SemaphoreHandle,
    waiter_attempted: AtomicU32,
    holder_releasing: AtomicU32,
    contention_observed: AtomicU32,
    holder_result: AtomicU32,
    waiter_result: AtomicU32,
}

/// Install the unique HAL-owned KM/RKP and TRNG capabilities before radio initialization.
#[cfg(target_arch = "riscv32")]
pub(crate) fn install_hardware_crypto(
    km: Km<'static>,
    spacc: Spacc<'static>,
    pke: Pke<'static>,
    trng: Trng<'static>,
) -> Result<(), CryptoError> {
    let mutex =
        hisi_rf_rtos_driver::mutex_create().map_err(|_| CryptoError::Backend(0xffff_1002))?;
    let Some(storage) = CRYPTO_STORAGE.try_init(Ws63CryptoStorage::new()) else {
        // SAFETY: this handle was just created above and has not escaped.
        let _ = unsafe { hisi_rf_rtos_driver::mutex_destroy(mutex) };
        return Err(CryptoError::InvalidValue);
    };
    let Some(service) = CRYPTO_CELL.try_init(CryptoService {
        backend: Ws63Crypto::new(Ws63CryptoResources::new(km, spacc, trng, storage)),
        p256: Ws63P256::new(pke),
        mutex,
    }) else {
        // SAFETY: this handle was just created above and has not escaped.
        let _ = unsafe { hisi_rf_rtos_driver::mutex_destroy(mutex) };
        return Err(CryptoError::InvalidValue);
    };
    CRYPTO_SERVICE.store(service, Ordering::Release);
    Ok(())
}

#[cfg(target_arch = "riscv32")]
fn with_crypto_service<T>(
    operation: impl FnOnce(&CryptoService) -> Result<T, CryptoError>,
) -> Result<T, CryptoError> {
    let service = CRYPTO_SERVICE.load(Ordering::Acquire);
    let service = if service.is_null() {
        return Err(CryptoError::Unsupported);
    } else {
        // SAFETY: Acquire observed a pointer published only after StaticCell
        // initialization. The runtime mutex below serializes the backend's
        // intentionally !Sync hardware state for the firmware lifetime.
        unsafe { &*service }
    };
    let lock = hisi_rf_rtos_driver::mutex_lock(service.mutex, WaitTimeout::from_millis(100))
        .map_err(|_| CryptoError::Backend(0xffff_1003));
    match lock {
        Err(error) => return Err(error),
        Ok(WaitOutcome::Acquired) => {}
        Ok(WaitOutcome::TimedOut) => return Err(CryptoError::Backend(0xffff_1004)),
    }
    let result = operation(service);
    let unlock = hisi_rf_rtos_driver::mutex_unlock(service.mutex)
        .map_err(|_| CryptoError::Backend(0xffff_1005));
    match (result, unlock) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) | (Ok(_), Err(error)) => Err(error),
    }
}

#[cfg(target_arch = "riscv32")]
fn with_hardware_crypto<T>(
    operation: impl FnOnce(&Ws63Crypto<'static>) -> Result<T, CryptoError>,
) -> Result<T, CryptoError> {
    with_crypto_service(|service| operation(&service.backend))
}

#[cfg(target_arch = "riscv32")]
pub(super) fn p256_point_mul_hardware(
    point: &P256AffinePoint,
    scalar: &[u8; 32],
    output: &mut P256AffinePoint,
) -> Result<(), CryptoError> {
    P256_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let started = crate::uapi::monotonic_ms();
    let result = with_crypto_service(|service| {
        service
            .p256
            .session(&service.backend)
            .point_mul(point, scalar, output)
    });
    let elapsed =
        u32::try_from(crate::uapi::monotonic_ms().wrapping_sub(started)).unwrap_or(u32::MAX);
    P256_TOTAL_MS.fetch_add(elapsed, Ordering::Relaxed);
    P256_MAX_MS.fetch_max(elapsed, Ordering::Relaxed);
    if result.is_err() {
        P256_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    result
}

#[cfg(target_arch = "riscv32")]
pub(super) fn p256_point_add_hardware(
    a: &P256AffinePoint,
    b: &P256AffinePoint,
    output: &mut P256PointResult,
) -> Result<(), CryptoError> {
    P256_REQUESTS.fetch_add(1, Ordering::Relaxed);
    P256_ADD_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let started = crate::uapi::monotonic_ms();
    let result = with_crypto_service(|service| {
        service
            .p256
            .session(&service.backend)
            .point_add(a, b, output)
    });
    let elapsed =
        u32::try_from(crate::uapi::monotonic_ms().wrapping_sub(started)).unwrap_or(u32::MAX);
    P256_TOTAL_MS.fetch_add(elapsed, Ordering::Relaxed);
    P256_MAX_MS.fetch_max(elapsed, Ordering::Relaxed);
    P256_ADD_TOTAL_MS.fetch_add(elapsed, Ordering::Relaxed);
    P256_ADD_MAX_MS.fetch_max(elapsed, Ordering::Relaxed);
    if result.is_err() {
        P256_FAILURES.fetch_add(1, Ordering::Relaxed);
        P256_ADD_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    result
}

#[cfg(target_arch = "riscv32")]
enum P256FieldOperation {
    Multiply,
    Square,
    Pow,
}

#[cfg(target_arch = "riscv32")]
fn record_p256_field_result(
    started: u64,
    operation: P256FieldOperation,
    result: &Result<(), CryptoError>,
) {
    let elapsed =
        u32::try_from(crate::uapi::monotonic_ms().wrapping_sub(started)).unwrap_or(u32::MAX);
    P256_FIELD_TOTAL_MS.fetch_add(elapsed, Ordering::Relaxed);
    P256_FIELD_MAX_MS.fetch_max(elapsed, Ordering::Relaxed);
    if result.is_err() {
        P256_FIELD_FAILURES.fetch_add(1, Ordering::Relaxed);
        match operation {
            P256FieldOperation::Multiply => {
                P256_FIELD_MUL_FAILURES.fetch_add(1, Ordering::Relaxed);
            }
            P256FieldOperation::Square => {
                P256_FIELD_SQUARE_FAILURES.fetch_add(1, Ordering::Relaxed);
            }
            P256FieldOperation::Pow => {
                P256_FIELD_POW_FAILURES.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(target_arch = "riscv32")]
pub(super) fn p256_field_mul_hardware(
    a: &P256FieldElement,
    b: &P256FieldElement,
    output: &mut P256FieldElement,
) -> Result<(), CryptoError> {
    P256_FIELD_REQUESTS.fetch_add(1, Ordering::Relaxed);
    P256_FIELD_MUL_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let started = crate::uapi::monotonic_ms();
    let result = with_crypto_service(|service| {
        service
            .p256
            .session(&service.backend)
            .field_mul(a, b, output)
    });
    record_p256_field_result(started, P256FieldOperation::Multiply, &result);
    result
}

#[cfg(target_arch = "riscv32")]
pub(super) fn p256_field_square_hardware(
    value: &P256FieldElement,
    output: &mut P256FieldElement,
) -> Result<(), CryptoError> {
    P256_FIELD_REQUESTS.fetch_add(1, Ordering::Relaxed);
    P256_FIELD_SQUARE_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let started = crate::uapi::monotonic_ms();
    let result = with_crypto_service(|service| {
        service
            .p256
            .session(&service.backend)
            .field_square(value, output)
    });
    record_p256_field_result(started, P256FieldOperation::Square, &result);
    result
}

#[cfg(target_arch = "riscv32")]
pub(super) fn p256_field_pow_hardware(
    base: &P256FieldElement,
    exponent: &[u8; 32],
    output: &mut P256FieldElement,
) -> Result<(), CryptoError> {
    P256_FIELD_REQUESTS.fetch_add(1, Ordering::Relaxed);
    P256_FIELD_POW_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let started = crate::uapi::monotonic_ms();
    let result = with_crypto_service(|service| {
        service
            .p256
            .session(&service.backend)
            .field_pow(base, exponent, output)
    });
    record_p256_field_result(started, P256FieldOperation::Pow, &result);
    result
}

#[cfg(target_arch = "riscv32")]
enum P256CurveOperation {
    Invert,
    Validate,
    YSquared,
}

#[cfg(target_arch = "riscv32")]
fn record_p256_curve_result(started: u64, operation: P256CurveOperation, failed: bool) {
    let elapsed =
        u32::try_from(crate::uapi::monotonic_ms().wrapping_sub(started)).unwrap_or(u32::MAX);
    P256_CURVE_TOTAL_MS.fetch_add(elapsed, Ordering::Relaxed);
    P256_CURVE_MAX_MS.fetch_max(elapsed, Ordering::Relaxed);
    if failed {
        P256_CURVE_FAILURES.fetch_add(1, Ordering::Relaxed);
        match operation {
            P256CurveOperation::Invert => {
                P256_CURVE_INVERT_FAILURES.fetch_add(1, Ordering::Relaxed);
            }
            P256CurveOperation::Validate => {
                P256_CURVE_VALIDATE_FAILURES.fetch_add(1, Ordering::Relaxed);
            }
            P256CurveOperation::YSquared => {
                P256_CURVE_Y2_FAILURES.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(target_arch = "riscv32")]
pub(super) fn p256_compute_y_squared_hardware(
    x: &P256FieldElement,
    output: &mut P256FieldElement,
) -> Result<(), CryptoError> {
    P256_CURVE_REQUESTS.fetch_add(1, Ordering::Relaxed);
    P256_CURVE_Y2_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let started = crate::uapi::monotonic_ms();
    let result = with_crypto_service(|service| {
        service
            .p256
            .session(&service.backend)
            .try_compute_y_squared(x, output)
    });
    record_p256_curve_result(started, P256CurveOperation::YSquared, result.is_err());
    result
}

#[cfg(target_arch = "riscv32")]
pub(super) fn p256_point_validate_hardware(point: &P256AffinePoint) -> Result<bool, CryptoError> {
    P256_CURVE_REQUESTS.fetch_add(1, Ordering::Relaxed);
    P256_CURVE_VALIDATE_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let started = crate::uapi::monotonic_ms();
    let result = with_crypto_service(|service| {
        service
            .p256
            .session(&service.backend)
            .try_point_is_on_curve(point)
    });
    record_p256_curve_result(started, P256CurveOperation::Validate, result.is_err());
    result
}

#[cfg(target_arch = "riscv32")]
pub(super) fn p256_point_invert_hardware(
    point: &P256AffinePoint,
    output: &mut P256AffinePoint,
) -> Result<(), CryptoError> {
    P256_CURVE_REQUESTS.fetch_add(1, Ordering::Relaxed);
    P256_CURVE_INVERT_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let started = crate::uapi::monotonic_ms();
    let result = with_crypto_service(|service| {
        service
            .p256
            .session(&service.backend)
            .try_point_invert(point, output)
    });
    record_p256_curve_result(started, P256CurveOperation::Invert, result.is_err());
    result
}

/// Fill bytes from the installed hardware TRNG without software fallback.
#[cfg(target_arch = "riscv32")]
pub(crate) fn fill_hardware_entropy(output: &mut [u8]) -> Result<(), CryptoError> {
    if output.is_empty() {
        return Ok(());
    }
    ENTROPY_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let result = with_hardware_crypto(|backend| backend.fill_entropy(output));
    if result.is_ok() {
        ENTROPY_BYTES.fetch_add(output.len() as u32, Ordering::Relaxed);
    } else {
        ENTROPY_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    result
}

/// Derive a WPA PMK through the explicitly selected WS63 RKP backend.
#[cfg(target_arch = "riscv32")]
pub(crate) fn derive_hardware_pbkdf2(
    password: &[u8],
    salt: &[u8],
    iterations: u32,
    output: &mut [u8; 32],
) -> Result<(), CryptoError> {
    PBKDF2_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let started = crate::uapi::monotonic_ms();
    let result =
        with_hardware_crypto(|backend| backend.derive_32(password, salt, iterations, output));
    let elapsed =
        u32::try_from(crate::uapi::monotonic_ms().wrapping_sub(started)).unwrap_or(u32::MAX);
    PBKDF2_TOTAL_MS.fetch_add(elapsed, Ordering::Relaxed);
    PBKDF2_MAX_MS.fetch_max(elapsed, Ordering::Relaxed);
    if result.is_err() {
        PBKDF2_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    result
}

#[cfg(target_arch = "riscv32")]
fn hash_hardware<const N: usize>(parts: &[&[u8]], output: &mut [u8; N]) -> Result<(), CryptoError>
where
    Ws63Crypto<'static>: TryHash<N>,
{
    HASH_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let started = crate::uapi::monotonic_ms();
    let result = with_hardware_crypto(|backend| TryHash::<N>::hash(backend, parts, output));
    let elapsed =
        u32::try_from(crate::uapi::monotonic_ms().wrapping_sub(started)).unwrap_or(u32::MAX);
    HASH_TOTAL_MS.fetch_add(elapsed, Ordering::Relaxed);
    HASH_MAX_MS.fetch_max(elapsed, Ordering::Relaxed);
    if result.is_err() {
        HASH_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    result
}

#[cfg(target_arch = "riscv32")]
fn mac_hardware<const N: usize>(
    key: &[u8],
    parts: &[&[u8]],
    output: &mut [u8; N],
) -> Result<(), CryptoError>
where
    Ws63Crypto<'static>: TryMac<N>,
{
    MAC_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let started = crate::uapi::monotonic_ms();
    let result = with_hardware_crypto(|backend| TryMac::<N>::mac(backend, key, parts, output));
    let elapsed =
        u32::try_from(crate::uapi::monotonic_ms().wrapping_sub(started)).unwrap_or(u32::MAX);
    MAC_TOTAL_MS.fetch_add(elapsed, Ordering::Relaxed);
    MAC_MAX_MS.fetch_max(elapsed, Ordering::Relaxed);
    if result.is_err() {
        MAC_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    result
}

#[cfg(target_arch = "riscv32")]
fn cipher_hardware(
    key: &[u8],
    input: &[u8; 16],
    output: &mut [u8; 16],
    decrypt: bool,
) -> Result<(), CryptoError> {
    CIPHER_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let started = crate::uapi::monotonic_ms();
    let result = with_hardware_crypto(|backend| {
        if decrypt {
            backend.decrypt_block(key, input, output)
        } else {
            backend.encrypt_block(key, input, output)
        }
    });
    let elapsed =
        u32::try_from(crate::uapi::monotonic_ms().wrapping_sub(started)).unwrap_or(u32::MAX);
    CIPHER_TOTAL_MS.fetch_add(elapsed, Ordering::Relaxed);
    CIPHER_MAX_MS.fetch_max(elapsed, Ordering::Relaxed);
    if result.is_err() {
        CIPHER_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    result
}

/// Return non-secret hardware entropy health counters for HIL diagnostics.
#[cfg(target_arch = "riscv32")]
pub(crate) fn hardware_entropy_diagnostic_snapshot() -> [u32; 4] {
    [
        u32::from(!CRYPTO_SERVICE.load(Ordering::Acquire).is_null()),
        ENTROPY_REQUESTS.load(Ordering::Relaxed),
        ENTROPY_BYTES.load(Ordering::Relaxed),
        ENTROPY_FAILURES.load(Ordering::Relaxed),
    ]
}

/// Return non-secret hardware PBKDF2 health counters for HIL diagnostics.
#[cfg(target_arch = "riscv32")]
pub(crate) fn hardware_pbkdf2_diagnostic_snapshot() -> [u32; 5] {
    [
        u32::from(!CRYPTO_SERVICE.load(Ordering::Acquire).is_null()),
        PBKDF2_REQUESTS.load(Ordering::Relaxed),
        PBKDF2_FAILURES.load(Ordering::Relaxed),
        PBKDF2_TOTAL_MS.load(Ordering::Relaxed),
        PBKDF2_MAX_MS.load(Ordering::Relaxed),
    ]
}

/// Return non-secret SPACC hash, HMAC, and recovery counters for HIL diagnostics.
#[cfg(target_arch = "riscv32")]
pub(crate) fn hardware_hash_diagnostic_snapshot() -> [u32; 10] {
    [
        HASH_REQUESTS.load(Ordering::Relaxed),
        HASH_FAILURES.load(Ordering::Relaxed),
        HASH_TOTAL_MS.load(Ordering::Relaxed),
        HASH_MAX_MS.load(Ordering::Relaxed),
        MAC_REQUESTS.load(Ordering::Relaxed),
        MAC_FAILURES.load(Ordering::Relaxed),
        MAC_TOTAL_MS.load(Ordering::Relaxed),
        MAC_MAX_MS.load(Ordering::Relaxed),
        HASH_RECOVERY_TESTS.load(Ordering::Relaxed),
        HASH_RECOVERY_FAILURES.load(Ordering::Relaxed),
    ]
}

/// Return non-secret SPACC AES and recovery counters for HIL diagnostics.
#[cfg(target_arch = "riscv32")]
pub(crate) fn hardware_cipher_diagnostic_snapshot() -> [u32; 6] {
    [
        CIPHER_REQUESTS.load(Ordering::Relaxed),
        CIPHER_FAILURES.load(Ordering::Relaxed),
        CIPHER_TOTAL_MS.load(Ordering::Relaxed),
        CIPHER_MAX_MS.load(Ordering::Relaxed),
        CIPHER_RECOVERY_TESTS.load(Ordering::Relaxed),
        CIPHER_RECOVERY_FAILURES.load(Ordering::Relaxed),
    ]
}

/// Return non-secret PKE P-256 point-operation counters.
#[cfg(target_arch = "riscv32")]
pub(crate) fn hardware_p256_diagnostic_snapshot() -> [u32; 8] {
    [
        P256_REQUESTS.load(Ordering::Relaxed),
        P256_FAILURES.load(Ordering::Relaxed),
        P256_TOTAL_MS.load(Ordering::Relaxed),
        P256_MAX_MS.load(Ordering::Relaxed),
        P256_ADD_REQUESTS.load(Ordering::Relaxed),
        P256_ADD_FAILURES.load(Ordering::Relaxed),
        P256_ADD_TOTAL_MS.load(Ordering::Relaxed),
        P256_ADD_MAX_MS.load(Ordering::Relaxed),
    ]
}

/// Return non-secret PKE P-256 fixed-field-operation counters.
#[cfg(target_arch = "riscv32")]
pub(crate) fn hardware_p256_field_diagnostic_snapshot() -> [u32; 10] {
    [
        P256_FIELD_REQUESTS.load(Ordering::Relaxed),
        P256_FIELD_FAILURES.load(Ordering::Relaxed),
        P256_FIELD_TOTAL_MS.load(Ordering::Relaxed),
        P256_FIELD_MAX_MS.load(Ordering::Relaxed),
        P256_FIELD_MUL_REQUESTS.load(Ordering::Relaxed),
        P256_FIELD_SQUARE_REQUESTS.load(Ordering::Relaxed),
        P256_FIELD_POW_REQUESTS.load(Ordering::Relaxed),
        P256_FIELD_MUL_FAILURES.load(Ordering::Relaxed),
        P256_FIELD_SQUARE_FAILURES.load(Ordering::Relaxed),
        P256_FIELD_POW_FAILURES.load(Ordering::Relaxed),
    ]
}

/// Return non-secret fixed P-256 curve-composition counters.
#[cfg(target_arch = "riscv32")]
pub(crate) fn hardware_p256_curve_diagnostic_snapshot() -> [u32; 10] {
    [
        P256_CURVE_REQUESTS.load(Ordering::Relaxed),
        P256_CURVE_FAILURES.load(Ordering::Relaxed),
        P256_CURVE_TOTAL_MS.load(Ordering::Relaxed),
        P256_CURVE_MAX_MS.load(Ordering::Relaxed),
        P256_CURVE_INVERT_REQUESTS.load(Ordering::Relaxed),
        P256_CURVE_VALIDATE_REQUESTS.load(Ordering::Relaxed),
        P256_CURVE_Y2_REQUESTS.load(Ordering::Relaxed),
        P256_CURVE_INVERT_FAILURES.load(Ordering::Relaxed),
        P256_CURVE_VALIDATE_FAILURES.load(Ordering::Relaxed),
        P256_CURVE_Y2_FAILURES.load(Ordering::Relaxed),
    ]
}

/// Return real cross-task CryptoService contention evidence.
#[cfg(all(target_arch = "riscv32", feature = "rf-crypto-contention-diag"))]
pub(crate) fn hardware_crypto_contention_diagnostic_snapshot() -> [u32; 5] {
    [
        CRYPTO_CONTENTION_TESTS.load(Ordering::Relaxed),
        CRYPTO_CONTENTION_FAILURES.load(Ordering::Relaxed),
        CRYPTO_CONTENTION_OBSERVED.load(Ordering::Relaxed),
        CRYPTO_CONTENTION_HOLDER_COMPLETIONS.load(Ordering::Relaxed),
        CRYPTO_CONTENTION_WAITER_COMPLETIONS.load(Ordering::Relaxed),
    ]
}

#[cfg(not(target_arch = "riscv32"))]
pub(crate) fn install_hardware_crypto(
    _km: Km<'static>,
    _spacc: Spacc<'static>,
    _pke: Pke<'static>,
    _trng: Trng<'static>,
) -> Result<(), CryptoError> {
    Err(CryptoError::Unsupported)
}

#[cfg(not(target_arch = "riscv32"))]
pub(crate) fn fill_hardware_entropy(_output: &mut [u8]) -> Result<(), CryptoError> {
    Err(CryptoError::Unsupported)
}

#[cfg(not(target_arch = "riscv32"))]
pub(crate) fn derive_hardware_pbkdf2(
    _password: &[u8],
    _salt: &[u8],
    _iterations: u32,
    _output: &mut [u8; 32],
) -> Result<(), CryptoError> {
    Err(CryptoError::Unsupported)
}

#[cfg(not(target_arch = "riscv32"))]
pub(crate) fn hardware_pbkdf2_diagnostic_snapshot() -> [u32; 5] {
    [0; 5]
}

#[cfg(not(target_arch = "riscv32"))]
pub(crate) fn hardware_hash_diagnostic_snapshot() -> [u32; 10] {
    [0; 10]
}

#[cfg(not(target_arch = "riscv32"))]
pub(crate) fn hardware_cipher_diagnostic_snapshot() -> [u32; 6] {
    [0; 6]
}

#[cfg(not(target_arch = "riscv32"))]
pub(crate) fn hardware_p256_diagnostic_snapshot() -> [u32; 8] {
    [0; 8]
}

#[cfg(not(target_arch = "riscv32"))]
pub(crate) fn hardware_p256_field_diagnostic_snapshot() -> [u32; 10] {
    [0; 10]
}

#[cfg(not(target_arch = "riscv32"))]
pub(crate) fn hardware_p256_curve_diagnostic_snapshot() -> [u32; 10] {
    [0; 10]
}

#[cfg(all(
    target_arch = "riscv32",
    any(feature = "wifi-wpa2-personal", feature = "upstream-supplicant-port")
))]
pub(crate) fn ws63_pbkdf2_self_test() -> Result<(), CryptoError> {
    const PBKDF2_EXPECTED: [u8; 32] = [
        0xf4, 0x2c, 0x6f, 0xc5, 0x2d, 0xf0, 0xeb, 0xef, 0x9e, 0xbb, 0x4b, 0x90, 0xb3, 0x8a, 0x5f,
        0x90, 0x2e, 0x83, 0xfe, 0x1b, 0x13, 0x5a, 0x70, 0xe2, 0x3a, 0xed, 0x76, 0x2e, 0x97, 0x10,
        0xa1, 0x2e,
    ];
    let mut pmk = [0; 32];
    derive_hardware_pbkdf2(b"password", b"IEEE", 4096, &mut pmk)?;
    if pmk != PBKDF2_EXPECTED {
        return Err(CryptoError::Backend(0xffff_0103));
    }
    Ok(())
}

#[cfg(all(
    target_arch = "riscv32",
    any(feature = "wifi-wpa2-personal", feature = "upstream-supplicant-port")
))]
pub(crate) fn ws63_hash_self_test() -> Result<(), CryptoError> {
    const SHA1_EXPECTED: [u8; 20] = [
        0xa9, 0x99, 0x3e, 0x36, 0x47, 0x06, 0x81, 0x6a, 0xba, 0x3e, 0x25, 0x71, 0x78, 0x50, 0xc2,
        0x6c, 0x9c, 0xd0, 0xd8, 0x9d,
    ];
    const SHA256_EXPECTED: [u8; 32] = [
        0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22,
        0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00,
        0x15, 0xad,
    ];
    const HMAC_SHA1_EXPECTED: [u8; 20] = [
        0xb6, 0x17, 0x31, 0x86, 0x55, 0x05, 0x72, 0x64, 0xe2, 0x8b, 0xc0, 0xb6, 0xfb, 0x37, 0x8c,
        0x8e, 0xf1, 0x46, 0xbe, 0x00,
    ];
    const HMAC_SHA256_EXPECTED: [u8; 32] = [
        0xb0, 0x34, 0x4c, 0x61, 0xd8, 0xdb, 0x38, 0x53, 0x5c, 0xa8, 0xaf, 0xce, 0xaf, 0x0b, 0xf1,
        0x2b, 0x88, 0x1d, 0xc2, 0x00, 0xc9, 0x83, 0x3d, 0xa7, 0x26, 0xe9, 0x37, 0x6c, 0x2e, 0x32,
        0xcf, 0xf7,
    ];

    let abc = [&b"abc"[..]];
    let mut sha1 = [0; 20];
    hash_hardware(&abc, &mut sha1)?;
    if sha1 != SHA1_EXPECTED {
        return Err(CryptoError::Backend(0xffff_0111));
    }
    let mut sha256 = [0; 32];
    hash_hardware(&abc, &mut sha256)?;
    if sha256 != SHA256_EXPECTED {
        return Err(CryptoError::Backend(0xffff_0112));
    }
    let parts = [&b"Hi There"[..]];
    mac_hardware(&[0x0b; 20], &parts, &mut sha1)?;
    if sha1 != HMAC_SHA1_EXPECTED {
        return Err(CryptoError::Backend(0xffff_0113));
    }
    mac_hardware(&[0x0b; 20], &parts, &mut sha256)?;
    if sha256 != HMAC_SHA256_EXPECTED {
        return Err(CryptoError::Backend(0xffff_0114));
    }
    Ok(())
}

#[cfg(all(target_arch = "riscv32", feature = "upstream-supplicant-wpa3"))]
pub(crate) fn ws63_p256_self_test() -> Result<(), CryptoError> {
    const GENERATOR: P256AffinePoint = P256AffinePoint::new(
        [
            0x6b, 0x17, 0xd1, 0xf2, 0xe1, 0x2c, 0x42, 0x47, 0xf8, 0xbc, 0xe6, 0xe5, 0x63, 0xa4,
            0x40, 0xf2, 0x77, 0x03, 0x7d, 0x81, 0x2d, 0xeb, 0x33, 0xa0, 0xf4, 0xa1, 0x39, 0x45,
            0xd8, 0x98, 0xc2, 0x96,
        ],
        [
            0x4f, 0xe3, 0x42, 0xe2, 0xfe, 0x1a, 0x7f, 0x9b, 0x8e, 0xe7, 0xeb, 0x4a, 0x7c, 0x0f,
            0x9e, 0x16, 0x2b, 0xce, 0x33, 0x57, 0x6b, 0x31, 0x5e, 0xce, 0xcb, 0xb6, 0x40, 0x68,
            0x37, 0xbf, 0x51, 0xf5,
        ],
    );
    let mut scalar = [0u8; 32];
    scalar[31] = 1;
    let mut output = P256AffinePoint::new([0; 32], [0; 32]);
    p256_point_mul_hardware(&GENERATOR, &scalar, &mut output)?;
    scalar.zeroize();
    if output != GENERATOR {
        output.x.zeroize();
        output.y.zeroize();
        return Err(CryptoError::Backend(0xffff_0301));
    }
    output.x.zeroize();
    output.y.zeroize();

    const DOUBLE_GENERATOR: P256AffinePoint = P256AffinePoint::new(
        [
            0x7c, 0xf2, 0x7b, 0x18, 0x8d, 0x03, 0x4f, 0x7e, 0x8a, 0x52, 0x38, 0x03, 0x04, 0xb5,
            0x1a, 0xc3, 0xc0, 0x89, 0x69, 0xe2, 0x77, 0xf2, 0x1b, 0x35, 0xa6, 0x0b, 0x48, 0xfc,
            0x47, 0x66, 0x99, 0x78,
        ],
        [
            0x07, 0x77, 0x55, 0x10, 0xdb, 0x8e, 0xd0, 0x40, 0x29, 0x3d, 0x9a, 0xc6, 0x9f, 0x74,
            0x30, 0xdb, 0xba, 0x7d, 0xad, 0xe6, 0x3c, 0xe9, 0x82, 0x29, 0x9e, 0x04, 0xb7, 0x9d,
            0x22, 0x78, 0x73, 0xd1,
        ],
    );
    let mut sum = P256PointResult::Infinity;
    p256_point_add_hardware(&GENERATOR, &GENERATOR, &mut sum)?;
    if sum != P256PointResult::Affine(DOUBLE_GENERATOR) {
        return Err(CryptoError::Backend(0xffff_0302));
    }

    const TRIPLE_GENERATOR: P256AffinePoint = P256AffinePoint::new(
        [
            0x5e, 0xcb, 0xe4, 0xd1, 0xa6, 0x33, 0x0a, 0x44, 0xc8, 0xf7, 0xef, 0x95, 0x1d, 0x4b,
            0xf1, 0x65, 0xe6, 0xc6, 0xb7, 0x21, 0xef, 0xad, 0xa9, 0x85, 0xfb, 0x41, 0x66, 0x1b,
            0xc6, 0xe7, 0xfd, 0x6c,
        ],
        [
            0x87, 0x34, 0x64, 0x0c, 0x49, 0x98, 0xff, 0x7e, 0x37, 0x4b, 0x06, 0xce, 0x1a, 0x64,
            0xa2, 0xec, 0xd8, 0x2a, 0xb0, 0x36, 0x38, 0x4f, 0xb8, 0x3d, 0x9a, 0x79, 0xb1, 0x27,
            0xa2, 0x7d, 0x50, 0x32,
        ],
    );
    p256_point_add_hardware(&GENERATOR, &DOUBLE_GENERATOR, &mut sum)?;
    if sum != P256PointResult::Affine(TRIPLE_GENERATOR) {
        return Err(CryptoError::Backend(0xffff_0303));
    }

    const NEGATIVE_GENERATOR: P256AffinePoint = P256AffinePoint::new(
        GENERATOR.x,
        [
            0xb0, 0x1c, 0xbd, 0x1c, 0x01, 0xe5, 0x80, 0x65, 0x71, 0x18, 0x14, 0xb5, 0x83, 0xf0,
            0x61, 0xe9, 0xd4, 0x31, 0xcc, 0xa9, 0x94, 0xce, 0xa1, 0x31, 0x34, 0x49, 0xbf, 0x97,
            0xc8, 0x40, 0xae, 0x0a,
        ],
    );
    p256_point_add_hardware(&GENERATOR, &NEGATIVE_GENERATOR, &mut sum)?;
    if sum != P256PointResult::Infinity {
        return Err(CryptoError::Backend(0xffff_0304));
    }

    let field_x = P256FieldElement::try_from_be_bytes(GENERATOR.x)?;
    let field_y = P256FieldElement::try_from_be_bytes(GENERATOR.y)?;
    let mut field_output = P256FieldElement::ZERO;
    p256_field_mul_hardware(&field_x, &field_y, &mut field_output)?;
    if field_output.as_be_bytes()
        != &[
            0x82, 0x3c, 0xd1, 0x5f, 0x6d, 0xd3, 0xc7, 0x19, 0x33, 0x56, 0x50, 0x64, 0x51, 0x3a,
            0x6b, 0x2b, 0xd1, 0x83, 0xe5, 0x54, 0xc6, 0xa0, 0x86, 0x22, 0xf7, 0x13, 0xeb, 0xbb,
            0xfa, 0xce, 0x98, 0xbe,
        ]
    {
        return Err(CryptoError::Backend(0xffff_0305));
    }
    p256_field_square_hardware(&field_x, &mut field_output)?;
    if field_output.as_be_bytes()
        != &[
            0x98, 0xf6, 0xb8, 0x4d, 0x29, 0xbe, 0xf2, 0xb2, 0x81, 0x81, 0x9a, 0x5e, 0x0e, 0x36,
            0x90, 0xd8, 0x33, 0xb6, 0x99, 0x49, 0x5d, 0x69, 0x4d, 0xd1, 0x00, 0x2a, 0xe5, 0x6c,
            0x42, 0x6b, 0x3f, 0x8c,
        ]
    {
        return Err(CryptoError::Backend(0xffff_0306));
    }
    const INVERSE_EXPONENT: [u8; 32] = [
        0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xfd,
    ];
    p256_field_pow_hardware(&field_x, &INVERSE_EXPONENT, &mut field_output)?;
    if field_output.as_be_bytes()
        != &[
            0xe0, 0x60, 0xcb, 0xb0, 0x88, 0x70, 0x6d, 0x5d, 0x24, 0x93, 0x69, 0x33, 0xb6, 0x9b,
            0x16, 0xab, 0x70, 0x7d, 0x65, 0x62, 0x73, 0x74, 0x4b, 0x65, 0x66, 0x4c, 0x49, 0xe5,
            0x77, 0xf3, 0x52, 0x38,
        ]
    {
        return Err(CryptoError::Backend(0xffff_0307));
    }
    if !p256_point_validate_hardware(&GENERATOR)? {
        return Err(CryptoError::Backend(0xffff_0308));
    }
    let mut y_squared = P256FieldElement::ZERO;
    p256_compute_y_squared_hardware(&field_x, &mut y_squared)?;
    p256_field_square_hardware(&field_y, &mut field_output)?;
    if y_squared != field_output {
        return Err(CryptoError::Backend(0xffff_0309));
    }
    let mut inverted = P256AffinePoint::new([0; 32], [0; 32]);
    p256_point_invert_hardware(&GENERATOR, &mut inverted)?;
    if inverted != NEGATIVE_GENERATOR {
        inverted.x.zeroize();
        inverted.y.zeroize();
        return Err(CryptoError::Backend(0xffff_030a));
    }
    inverted.x.zeroize();
    inverted.y.zeroize();
    if p256_point_validate_hardware(&P256AffinePoint::new([0; 32], [0; 32]))? {
        return Err(CryptoError::Backend(0xffff_030b));
    }
    Ok(())
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "rf-eloop-diag",
    any(feature = "wifi-wpa2-personal", feature = "upstream-supplicant-port")
))]
pub(crate) fn ws63_hash_fault_recovery_self_test() -> Result<(), CryptoError> {
    HASH_RECOVERY_TESTS.fetch_add(1, Ordering::Relaxed);
    let result = with_hardware_crypto(|backend| backend.diagnostic_lock_timeout_recovery());
    if result.is_err() {
        HASH_RECOVERY_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    result
}

#[cfg(all(
    target_arch = "riscv32",
    feature = "rf-eloop-diag",
    any(feature = "wifi-wpa2-personal", feature = "upstream-supplicant-port")
))]
pub(crate) fn ws63_cipher_fault_recovery_self_test() -> Result<(), CryptoError> {
    CIPHER_RECOVERY_TESTS.fetch_add(1, Ordering::Relaxed);
    let result = with_hardware_crypto(|backend| backend.diagnostic_cipher_recovery());
    if result.is_err() {
        CIPHER_RECOVERY_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    result
}

#[cfg(all(target_arch = "riscv32", feature = "rf-crypto-contention-diag"))]
fn contention_runtime_error(code: u32) -> CryptoError {
    CryptoError::Backend(0xffff_1200 | code)
}

#[cfg(all(target_arch = "riscv32", feature = "rf-crypto-contention-diag"))]
fn signal_contention_done(
    context: &CryptoContentionContext,
    holder: bool,
    result: Result<(), CryptoError>,
) {
    let code = result.map_or_else(|error| error.code(), |()| 0);
    if holder {
        context.holder_result.store(code, Ordering::Release);
    } else {
        context.waiter_result.store(code, Ordering::Release);
    }
    let _ = hisi_rf_rtos_driver::semaphore_up(context.done);
}

#[cfg(all(target_arch = "riscv32", feature = "rf-crypto-contention-diag"))]
extern "C" fn crypto_contention_holder(argument: *mut core::ffi::c_void) -> *mut core::ffi::c_void {
    // SAFETY: the argument points to a StaticCell value that lives for the
    // firmware lifetime and this diagnostic waits for both tasks to finish.
    let context = unsafe { &*argument.cast::<CryptoContentionContext>() };
    let result = with_crypto_service(|service| {
        hisi_rf_rtos_driver::semaphore_up(context.start)
            .map_err(|_| contention_runtime_error(1))?;
        hisi_rf_rtos_driver::sleep_ms(core::num::NonZeroU32::new(10).unwrap())
            .map_err(|_| contention_runtime_error(2))?;
        if context.waiter_attempted.load(Ordering::Acquire) == 0 {
            return Err(contention_runtime_error(3));
        }
        context.contention_observed.store(1, Ordering::Release);

        const SHA256_EXPECTED: [u8; 32] = [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad,
        ];
        let mut output = [0; 32];
        let result = TryHash::<32>::hash(&service.backend, &[&b"abc"[..]], &mut output);
        let matches = output == SHA256_EXPECTED;
        output.zeroize();
        context.holder_releasing.store(1, Ordering::Release);
        result?;
        matches.then_some(()).ok_or(contention_runtime_error(4))
    });
    if result.is_ok() {
        CRYPTO_CONTENTION_HOLDER_COMPLETIONS.fetch_add(1, Ordering::Relaxed);
    }
    signal_contention_done(context, true, result);
    core::ptr::null_mut()
}

#[cfg(all(target_arch = "riscv32", feature = "rf-crypto-contention-diag"))]
extern "C" fn crypto_contention_waiter(argument: *mut core::ffi::c_void) -> *mut core::ffi::c_void {
    // SAFETY: see `crypto_contention_holder`; both entries receive the same
    // immutable synchronization context.
    let context = unsafe { &*argument.cast::<CryptoContentionContext>() };
    let result = (|| {
        match hisi_rf_rtos_driver::semaphore_down(context.start, WaitTimeout::from_millis(500))
            .map_err(|_| contention_runtime_error(5))?
        {
            WaitOutcome::Acquired => {}
            WaitOutcome::TimedOut => return Err(contention_runtime_error(6)),
        }
        context.waiter_attempted.store(1, Ordering::Release);

        const KEY: [u8; 16] = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf,
            0x4f, 0x3c,
        ];
        const PLAIN: [u8; 16] = [
            0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96, 0xe9, 0x3d, 0x7e, 0x11, 0x73, 0x93,
            0x17, 0x2a,
        ];
        const CIPHER: [u8; 16] = [
            0x3a, 0xd7, 0x7b, 0xb4, 0x0d, 0x7a, 0x36, 0x60, 0xa8, 0x9e, 0xca, 0xf3, 0x24, 0x66,
            0xef, 0x97,
        ];
        let mut output = [0; 16];
        cipher_hardware(&KEY, &PLAIN, &mut output, false)?;
        let matches = output == CIPHER;
        output.zeroize();
        if context.holder_releasing.load(Ordering::Acquire) == 0 {
            return Err(contention_runtime_error(7));
        }
        matches.then_some(()).ok_or(contention_runtime_error(8))
    })();
    if result.is_ok() {
        CRYPTO_CONTENTION_WAITER_COMPLETIONS.fetch_add(1, Ordering::Relaxed);
    }
    signal_contention_done(context, false, result);
    core::ptr::null_mut()
}

#[cfg(all(target_arch = "riscv32", feature = "rf-crypto-contention-diag"))]
pub(crate) fn ws63_crypto_contention_self_test() -> Result<(), CryptoError> {
    CRYPTO_CONTENTION_TESTS.fetch_add(1, Ordering::Relaxed);
    let result = (|| {
        let start =
            hisi_rf_rtos_driver::semaphore_create(0).map_err(|_| contention_runtime_error(9))?;
        let done = match hisi_rf_rtos_driver::semaphore_create(0) {
            Ok(done) => done,
            Err(_) => {
                // SAFETY: no task has received `start` yet.
                let _ = unsafe { hisi_rf_rtos_driver::semaphore_destroy(start) };
                return Err(contention_runtime_error(10));
            }
        };
        let Some(context) = CRYPTO_CONTENTION_CONTEXT.try_init(CryptoContentionContext {
            start,
            done,
            waiter_attempted: AtomicU32::new(0),
            holder_releasing: AtomicU32::new(0),
            contention_observed: AtomicU32::new(0),
            holder_result: AtomicU32::new(u32::MAX),
            waiter_result: AtomicU32::new(u32::MAX),
        }) else {
            // SAFETY: neither handle escaped to a task.
            let _ = unsafe { hisi_rf_rtos_driver::semaphore_destroy(start) };
            let _ = unsafe { hisi_rf_rtos_driver::semaphore_destroy(done) };
            return Err(contention_runtime_error(11));
        };
        let argument = core::ptr::from_ref(context).cast_mut().cast();
        hisi_rf_rtos_driver::spawn(
            crypto_contention_holder,
            argument,
            hisi_rf_rtos_driver::TaskConfig {
                stack_size: core::num::NonZeroUsize::new(4 * 1024).unwrap(),
                priority: 10,
            },
        )
        .map_err(|_| contention_runtime_error(12))?;
        let waiter_spawn = hisi_rf_rtos_driver::spawn(
            crypto_contention_waiter,
            argument,
            hisi_rf_rtos_driver::TaskConfig {
                stack_size: core::num::NonZeroUsize::new(4 * 1024).unwrap(),
                priority: 10,
            },
        );

        let completions = if waiter_spawn.is_ok() { 2 } else { 1 };
        for _ in 0..completions {
            match hisi_rf_rtos_driver::semaphore_down(done, WaitTimeout::from_millis(1_000))
                .map_err(|_| contention_runtime_error(13))?
            {
                WaitOutcome::Acquired => {}
                WaitOutcome::TimedOut => return Err(contention_runtime_error(14)),
            }
        }
        if waiter_spawn.is_err() {
            return Err(contention_runtime_error(15));
        }

        // SAFETY: both tasks signalled completion after their last access to
        // either semaphore. They may still be in the return trampoline, but no
        // longer retain or dereference these handles.
        unsafe {
            hisi_rf_rtos_driver::semaphore_destroy(start)
                .map_err(|_| contention_runtime_error(16))?;
            hisi_rf_rtos_driver::semaphore_destroy(done)
                .map_err(|_| contention_runtime_error(17))?;
        }
        if context.contention_observed.load(Ordering::Acquire) == 0
            || context.holder_result.load(Ordering::Acquire) != 0
            || context.waiter_result.load(Ordering::Acquire) != 0
        {
            return Err(contention_runtime_error(18));
        }
        CRYPTO_CONTENTION_OBSERVED.fetch_add(1, Ordering::Relaxed);
        Ok(())
    })();
    if result.is_err() {
        CRYPTO_CONTENTION_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    result
}

#[cfg(all(target_arch = "riscv32", feature = "wifi-wpa2-personal"))]
pub(crate) fn ws63_security_self_test() -> Result<(), CryptoError> {
    const KEY: [u8; 16] = [
        0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f,
        0x3c,
    ];
    const PLAIN: [u8; 16] = [
        0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96, 0xe9, 0x3d, 0x7e, 0x11, 0x73, 0x93, 0x17,
        0x2a,
    ];
    const CIPHER: [u8; 16] = [
        0x3a, 0xd7, 0x7b, 0xb4, 0x0d, 0x7a, 0x36, 0x60, 0xa8, 0x9e, 0xca, 0xf3, 0x24, 0x66, 0xef,
        0x97,
    ];
    const HMAC_SHA1_EXPECTED: [u8; 20] = [
        0xb6, 0x17, 0x31, 0x86, 0x55, 0x05, 0x72, 0x64, 0xe2, 0x8b, 0xc0, 0xb6, 0xfb, 0x37, 0x8c,
        0x8e, 0xf1, 0x46, 0xbe, 0x00,
    ];
    const HMAC_SHA256_EXPECTED: [u8; 32] = [
        0xb0, 0x34, 0x4c, 0x61, 0xd8, 0xdb, 0x38, 0x53, 0x5c, 0xa8, 0xaf, 0xce, 0xaf, 0x0b, 0xf1,
        0x2b, 0x88, 0x1d, 0xc2, 0x00, 0xc9, 0x83, 0x3d, 0xa7, 0x26, 0xe9, 0x37, 0x6c, 0x2e, 0x32,
        0xcf, 0xf7,
    ];
    ws63_pbkdf2_self_test()?;

    let parts = [&b"Hi There"[..]];
    let mut sha1 = [0; 20];
    mac_hardware(&[0x0b; 20], &parts, &mut sha1)?;
    if sha1 != HMAC_SHA1_EXPECTED {
        return Err(CryptoError::Backend(0xffff_0101));
    }
    let mut sha256 = [0; 32];
    mac_hardware(&[0x0b; 20], &parts, &mut sha256)?;
    if sha256 != HMAC_SHA256_EXPECTED {
        return Err(CryptoError::Backend(0xffff_0102));
    }

    let encrypt = unsafe { aes_encrypt_init(KEY.as_ptr(), KEY.len()) };
    if encrypt.is_null() {
        return Err(CryptoError::Backend(0xffff_0201));
    }
    let mut cipher = [0; 16];
    let result = unsafe { aes_encrypt(encrypt, PLAIN.as_ptr(), cipher.as_mut_ptr()) };
    unsafe { aes_encrypt_deinit(encrypt) };
    if result != 0 || cipher != CIPHER {
        return Err(CryptoError::Backend(0xffff_0202));
    }

    let decrypt = unsafe { aes_decrypt_init(KEY.as_ptr(), KEY.len()) };
    if decrypt.is_null() {
        return Err(CryptoError::Backend(0xffff_0203));
    }
    let mut plain = [0; 16];
    let result = unsafe { aes_decrypt(decrypt, CIPHER.as_ptr(), plain.as_mut_ptr()) };
    unsafe { aes_decrypt_deinit(decrypt) };
    if result != 0 || plain != PLAIN {
        return Err(CryptoError::Backend(0xffff_0204));
    }
    Ok(())
}

#[cfg(target_arch = "riscv32")]
#[repr(C)]
struct AesContext {
    key: [u8; 32],
    key_len: usize,
}

#[cfg(target_arch = "riscv32")]
unsafe fn aes_context_new(key: *const u8, key_len: usize) -> *mut core::ffi::c_void {
    if key.is_null() || !matches!(key_len, 16 | 24 | 32) {
        return core::ptr::null_mut();
    }
    let context = unsafe { os_zalloc(core::mem::size_of::<AesContext>()) }.cast::<AesContext>();
    if context.is_null() {
        return core::ptr::null_mut();
    }
    let mut material = [0; 32];
    material[..key_len].copy_from_slice(unsafe { core::slice::from_raw_parts(key, key_len) });
    unsafe {
        context.write(AesContext {
            key: material,
            key_len,
        });
    }
    material.zeroize();
    context.cast()
}

#[cfg(target_arch = "riscv32")]
unsafe fn aes_block(
    context: *mut core::ffi::c_void,
    input: *const u8,
    output: *mut u8,
    decrypt: bool,
) -> i32 {
    if context.is_null() || input.is_null() || output.is_null() {
        return -1;
    }
    let context = unsafe { &*context.cast::<AesContext>() };
    let input = unsafe { &*input.cast::<[u8; 16]>() };
    let output = unsafe { &mut *output.cast::<[u8; 16]>() };
    let key = &context.key[..context.key_len];
    let result = cipher_hardware(key, input, output, decrypt);
    result.map(|()| 0).unwrap_or(-1)
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn aes_encrypt_init(key: *const u8, key_len: usize) -> *mut core::ffi::c_void {
    unsafe { aes_context_new(key, key_len) }
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn aes_encrypt(
    context: *mut core::ffi::c_void,
    plain: *const u8,
    crypt: *mut u8,
) -> i32 {
    unsafe { aes_block(context, plain, crypt, false) }
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn aes_encrypt_deinit(context: *mut core::ffi::c_void) {
    if !context.is_null() {
        // SAFETY: the pointer was allocated and initialized by `aes_context_new`.
        unsafe { &mut *context.cast::<AesContext>() }.key.zeroize();
        unsafe { os_free(context) };
    }
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn aes_decrypt_init(key: *const u8, key_len: usize) -> *mut core::ffi::c_void {
    unsafe { aes_context_new(key, key_len) }
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn aes_decrypt(
    context: *mut core::ffi::c_void,
    crypt: *const u8,
    plain: *mut u8,
) -> i32 {
    unsafe { aes_block(context, crypt, plain, true) }
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
unsafe extern "C" fn aes_decrypt_deinit(context: *mut core::ffi::c_void) {
    if !context.is_null() {
        // SAFETY: the pointer was allocated and initialized by `aes_context_new`.
        unsafe { &mut *context.cast::<AesContext>() }.key.zeroize();
        unsafe { os_free(context) };
    }
}

#[cfg(target_arch = "riscv32")]
unsafe fn with_ffi_parts<const N: usize>(
    count: usize,
    addresses: *const *const u8,
    lengths: *const usize,
) -> Option<[&'static [u8]; N]> {
    if count > N || (count != 0 && (addresses.is_null() || lengths.is_null())) {
        return None;
    }
    let mut parts = [&[][..]; N];
    for (index, slot) in parts[..count].iter_mut().enumerate() {
        let address = unsafe { *addresses.add(index) };
        let length = unsafe { *lengths.add(index) };
        if length != 0 && address.is_null() {
            return None;
        }
        *slot = unsafe { core::slice::from_raw_parts(address, length) };
    }
    Some(parts)
}

#[cfg(target_arch = "riscv32")]
fn ffi_hmac<const N: usize>(
    key: *const u8,
    key_len: usize,
    count: usize,
    addresses: *const *const u8,
    lengths: *const usize,
    output: *mut u8,
    operation: impl FnOnce(&[u8], &[&[u8]], &mut [u8; N]) -> Result<(), CryptoError>,
) -> i32 {
    if (key_len != 0 && key.is_null()) || output.is_null() {
        return -1;
    }
    let Some(parts) = (unsafe { with_ffi_parts::<8>(count, addresses, lengths) }) else {
        return -1;
    };
    let key = unsafe { core::slice::from_raw_parts(key, key_len) };
    let output = unsafe { &mut *output.cast::<[u8; N]>() };
    operation(key, &parts[..count], output)
        .map(|()| 0)
        .unwrap_or(-1)
}

#[cfg(target_arch = "riscv32")]
fn ffi_digest<const N: usize>(
    count: usize,
    addresses: *const *const u8,
    lengths: *const usize,
    output: *mut u8,
    operation: impl FnOnce(&[&[u8]], &mut [u8; N]) -> Result<(), CryptoError>,
) -> i32 {
    if output.is_null() {
        return -1;
    }
    let Some(parts) = (unsafe { with_ffi_parts::<8>(count, addresses, lengths) }) else {
        return -1;
    };
    let output = unsafe { &mut *output.cast::<[u8; N]>() };
    operation(&parts[..count], output).map(|()| 0).unwrap_or(-1)
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
extern "C" fn hmac_sha1_vector(
    key: *const u8,
    key_len: usize,
    count: usize,
    addresses: *const *const u8,
    lengths: *const usize,
    output: *mut u8,
) -> i32 {
    ffi_hmac::<20>(
        key,
        key_len,
        count,
        addresses,
        lengths,
        output,
        mac_hardware,
    )
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
extern "C" fn hmac_sha1(
    key: *const u8,
    key_len: usize,
    data: *const u8,
    data_len: usize,
    output: *mut u8,
) -> i32 {
    hmac_sha1_vector(key, key_len, 1, &data, &data_len, output)
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
extern "C" fn hmac_sha256_vector(
    key: *const u8,
    key_len: usize,
    count: usize,
    addresses: *const *const u8,
    lengths: *const usize,
    output: *mut u8,
) -> i32 {
    ffi_hmac::<32>(
        key,
        key_len,
        count,
        addresses,
        lengths,
        output,
        mac_hardware,
    )
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
extern "C" fn hmac_sha256(
    key: *const u8,
    key_len: usize,
    data: *const u8,
    data_len: usize,
    output: *mut u8,
) -> i32 {
    hmac_sha256_vector(key, key_len, 1, &data, &data_len, output)
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
extern "C" fn sha1_vector(
    count: usize,
    addresses: *const *const u8,
    lengths: *const usize,
    output: *mut u8,
) -> i32 {
    ffi_digest::<20>(count, addresses, lengths, output, hash_hardware)
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
extern "C" fn sha256_vector(
    count: usize,
    addresses: *const *const u8,
    lengths: *const usize,
    output: *mut u8,
) -> i32 {
    ffi_digest::<32>(count, addresses, lengths, output, hash_hardware)
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
extern "C" fn pbkdf2_sha1(
    password: *const core::ffi::c_char,
    salt: *const u8,
    salt_len: usize,
    iterations: i32,
    output: *mut u8,
    output_len: usize,
) -> i32 {
    if password.is_null()
        || salt.is_null()
        || output.is_null()
        || iterations <= 0
        || iterations > i32::from(u16::MAX)
        || output_len != 32
    {
        return -1;
    }
    let mut password_len = 0;
    while password_len <= 63 && unsafe { *password.add(password_len) } != 0 {
        password_len += 1;
    }
    if password_len > 63 {
        return -1;
    }
    let password = unsafe { core::slice::from_raw_parts(password.cast(), password_len) };
    let salt = unsafe { core::slice::from_raw_parts(salt, salt_len) };
    let output = unsafe { &mut *output.cast::<[u8; 32]>() };
    let result = derive_hardware_pbkdf2(password, salt, iterations as u32, output);
    result.map(|()| 0).unwrap_or(-1)
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
extern "C" fn crypto_get_random(output: *mut core::ffi::c_void, length: usize) -> i32 {
    if length != 0 && output.is_null() {
        return -1;
    }
    let output = if length == 0 {
        &mut []
    } else {
        unsafe { core::slice::from_raw_parts_mut(output.cast(), length) }
    };
    fill_hardware_entropy(output).map(|()| 0).unwrap_or(-1)
}

#[cfg(target_arch = "riscv32")]
unsafe extern "C" {
    fn os_zalloc(size: usize) -> *mut core::ffi::c_void;
    fn os_free(pointer: *mut core::ffi::c_void);
}

#[cfg(all(test, feature = "wifi-security-rustcrypto"))]
mod tests {
    use hisi_crypto::{Pbkdf2HmacSha1, RustCryptoProvider, TryMac};

    #[test]
    fn derives_ieee_wpa_pmk() {
        let mut output = [0; 32];
        RustCryptoProvider
            .derive_32(b"password", b"IEEE", 4096, &mut output)
            .unwrap();
        assert_eq!(
            output,
            [
                0xf4, 0x2c, 0x6f, 0xc5, 0x2d, 0xf0, 0xeb, 0xef, 0x9e, 0xbb, 0x4b, 0x90, 0xb3, 0x8a,
                0x5f, 0x90, 0x2e, 0x83, 0xfe, 0x1b, 0x13, 0x5a, 0x70, 0xe2, 0x3a, 0xed, 0x76, 0x2e,
                0x97, 0x10, 0xa1, 0x2e,
            ]
        );
    }

    #[test]
    fn derives_rfc6070_block_prefix() {
        let mut output = [0; 32];
        RustCryptoProvider
            .derive_32(b"password", b"salt", 1, &mut output)
            .unwrap();
        assert_eq!(
            &output[..20],
            &[
                0x0c, 0x60, 0xc8, 0x0f, 0x96, 0x1f, 0x0e, 0x71, 0xf3, 0xa9, 0xb5, 0x24, 0xaf, 0x60,
                0x12, 0x06, 0x2f, 0xe0, 0x37, 0xa6,
            ]
        );
    }

    #[test]
    fn hmac_vectors_cover_multiple_parts() {
        let mut sha1 = [0; 20];
        let mut sha256 = [0; 32];
        let parts = [&b"Hi There"[..4], &b"Hi There"[4..]];
        TryMac::<20>::mac(&RustCryptoProvider, &[0x0b; 20], &parts, &mut sha1).unwrap();
        TryMac::<32>::mac(&RustCryptoProvider, &[0x0b; 20], &parts, &mut sha256).unwrap();
        assert_eq!(
            sha1,
            [
                0xb6, 0x17, 0x31, 0x86, 0x55, 0x05, 0x72, 0x64, 0xe2, 0x8b, 0xc0, 0xb6, 0xfb, 0x37,
                0x8c, 0x8e, 0xf1, 0x46, 0xbe, 0x00,
            ]
        );
        assert_eq!(
            sha256,
            [
                0xb0, 0x34, 0x4c, 0x61, 0xd8, 0xdb, 0x38, 0x53, 0x5c, 0xa8, 0xaf, 0xce, 0xaf, 0x0b,
                0xf1, 0x2b, 0x88, 0x1d, 0xc2, 0x00, 0xc9, 0x83, 0x3d, 0xa7, 0x26, 0xe9, 0x37, 0x6c,
                0x2e, 0x32, 0xcf, 0xf7,
            ]
        );
    }
}
