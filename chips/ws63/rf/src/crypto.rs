//! Internal Wi-Fi security provider boundary.
//!
//! The C supplicant must not make mbedTLS contexts part of the Rust API. WS63
//! uses token-owned WS63 RKP/SPACC capabilities where proven on silicon and
//! RustCrypto for the remaining portable AES primitive. `hisi-crypto` owns the
//! capability contract; RF owns only this WS63 service and C ABI shim.

pub(crate) use hisi_crypto::CryptoError;
#[cfg(all(
    target_arch = "riscv32",
    any(feature = "wifi-wpa2-personal", feature = "upstream-supplicant-port")
))]
use hisi_crypto::Pbkdf2HmacSha1;
#[cfg(target_arch = "riscv32")]
use hisi_crypto::{EntropySource, RustCryptoProvider, TryBlockCipher, TryHash, TryMac};
#[cfg(target_arch = "riscv32")]
use hisi_crypto_ws63::Ws63Crypto;
use hisi_hal::peripherals::{Km, Spacc, Trng};
#[cfg(target_arch = "riscv32")]
use hisi_rf_rtos_driver::{MutexHandle, WaitOutcome, WaitTimeout};
#[cfg(target_arch = "riscv32")]
use portable_atomic::{AtomicPtr, AtomicU32, Ordering};
#[cfg(target_arch = "riscv32")]
use static_cell::StaticCell;

#[cfg(feature = "upstream-supplicant-wpa3")]
#[path = "crypto_sae.rs"]
mod crypto_sae;

#[cfg(target_arch = "riscv32")]
struct CryptoService {
    backend: Ws63Crypto<'static>,
    mutex: MutexHandle,
}

#[cfg(target_arch = "riscv32")]
static CRYPTO_CELL: StaticCell<CryptoService> = StaticCell::new();
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

/// Install the unique HAL-owned KM/RKP and TRNG capabilities before radio initialization.
#[cfg(target_arch = "riscv32")]
pub(crate) fn install_hardware_crypto(
    km: Km<'static>,
    spacc: Spacc<'static>,
    trng: Trng<'static>,
) -> Result<(), CryptoError> {
    let mutex =
        hisi_rf_rtos_driver::mutex_create().map_err(|_| CryptoError::Backend(0xffff_1002))?;
    let Some(service) = CRYPTO_CELL.try_init(CryptoService {
        backend: Ws63Crypto::new(km, spacc, trng),
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
fn with_hardware_crypto<T>(
    operation: impl FnOnce(&Ws63Crypto<'static>) -> Result<T, CryptoError>,
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
    let result = operation(&service.backend);
    let unlock = hisi_rf_rtos_driver::mutex_unlock(service.mutex)
        .map_err(|_| CryptoError::Backend(0xffff_1005));
    match (result, unlock) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) | (Ok(_), Err(error)) => Err(error),
    }
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

#[cfg(not(target_arch = "riscv32"))]
pub(crate) fn install_hardware_crypto(
    _km: Km<'static>,
    _spacc: Spacc<'static>,
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
    let result = if decrypt {
        RustCryptoProvider.decrypt_block(key, input, output)
    } else {
        RustCryptoProvider.encrypt_block(key, input, output)
    };
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
