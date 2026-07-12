//! Internal Wi-Fi security provider boundary.
//!
//! The C supplicant must not make mbedTLS contexts part of the Rust API. WS63
//! uses the published unified-cipher UAPI; RustCrypto is the portable provider
//! and host-side known-answer oracle.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CryptoError(pub(crate) u32);

fn software_hmac_sha1(
    key: &[u8],
    parts: &[&[u8]],
    output: &mut [u8; 20],
) -> Result<(), CryptoError> {
    use hmac::{Mac, digest::KeyInit};
    let mut hmac = <hmac::Hmac<sha1::Sha1> as KeyInit>::new_from_slice(key)
        .map_err(|_| CryptoError(u32::MAX))?;
    for part in parts {
        hmac.update(part);
    }
    output.copy_from_slice(&hmac.finalize().into_bytes());
    Ok(())
}

fn software_hmac_sha256(
    key: &[u8],
    parts: &[&[u8]],
    output: &mut [u8; 32],
) -> Result<(), CryptoError> {
    use hmac::{Mac, digest::KeyInit};
    let mut hmac = <hmac::Hmac<sha2::Sha256> as KeyInit>::new_from_slice(key)
        .map_err(|_| CryptoError(u32::MAX))?;
    for part in parts {
        hmac.update(part);
    }
    output.copy_from_slice(&hmac.finalize().into_bytes());
    Ok(())
}

#[cfg(target_arch = "riscv32")]
fn software_sha1(parts: &[&[u8]], output: &mut [u8; 20]) {
    use sha1::Digest;
    let mut digest = sha1::Sha1::new();
    for part in parts {
        digest.update(part);
    }
    output.copy_from_slice(&digest.finalize());
}

#[cfg(target_arch = "riscv32")]
fn software_sha256(parts: &[&[u8]], output: &mut [u8; 32]) {
    use sha2::Digest;
    let mut digest = sha2::Sha256::new();
    for part in parts {
        digest.update(part);
    }
    output.copy_from_slice(&digest.finalize());
}

pub(crate) trait CryptoProvider {
    fn pbkdf2_hmac_sha1(
        &self,
        password: &[u8],
        salt: &[u8],
        iterations: u16,
        output: &mut [u8; 32],
    ) -> Result<(), CryptoError>;

    fn hmac_sha1(
        &self,
        key: &[u8],
        parts: &[&[u8]],
        output: &mut [u8; 20],
    ) -> Result<(), CryptoError>;

    fn hmac_sha256(
        &self,
        key: &[u8],
        parts: &[&[u8]],
        output: &mut [u8; 32],
    ) -> Result<(), CryptoError>;
}

#[cfg(feature = "wifi-security-rustcrypto")]
pub(crate) struct RustCryptoProvider;

#[cfg(feature = "wifi-security-rustcrypto")]
impl CryptoProvider for RustCryptoProvider {
    fn pbkdf2_hmac_sha1(
        &self,
        password: &[u8],
        salt: &[u8],
        iterations: u16,
        output: &mut [u8; 32],
    ) -> Result<(), CryptoError> {
        pbkdf2::pbkdf2_hmac::<sha1::Sha1>(password, salt, u32::from(iterations), output);
        Ok(())
    }

    fn hmac_sha1(
        &self,
        key: &[u8],
        parts: &[&[u8]],
        output: &mut [u8; 20],
    ) -> Result<(), CryptoError> {
        software_hmac_sha1(key, parts, output)
    }

    fn hmac_sha256(
        &self,
        key: &[u8],
        parts: &[&[u8]],
        output: &mut [u8; 32],
    ) -> Result<(), CryptoError> {
        software_hmac_sha256(key, parts, output)
    }
}

#[cfg(target_arch = "riscv32")]
pub(crate) struct Ws63CryptoProvider;

#[cfg(target_arch = "riscv32")]
impl CryptoProvider for Ws63CryptoProvider {
    fn pbkdf2_hmac_sha1(
        &self,
        password: &[u8],
        salt: &[u8],
        iterations: u16,
        output: &mut [u8; 32],
    ) -> Result<(), CryptoError> {
        let password_len = u32::try_from(password.len()).map_err(|_| CryptoError(u32::MAX))?;
        let salt_len = u32::try_from(salt.len()).map_err(|_| CryptoError(u32::MAX))?;
        let parameters = Pbkdf2Parameters {
            hash_type: HMAC_SHA1,
            password: password.as_ptr().cast_mut(),
            password_len,
            salt: salt.as_ptr().cast_mut(),
            salt_len,
            iterations,
        };
        let result = unsafe {
            uapi_drv_cipher_pbkdf2(&parameters, output.as_mut_ptr(), output.len() as u32)
        };
        if result == 0 {
            Ok(())
        } else {
            Err(CryptoError(result))
        }
    }

    fn hmac_sha1(
        &self,
        key: &[u8],
        parts: &[&[u8]],
        output: &mut [u8; 20],
    ) -> Result<(), CryptoError> {
        software_hmac_sha1(key, parts, output)
    }

    fn hmac_sha256(
        &self,
        key: &[u8],
        parts: &[&[u8]],
        output: &mut [u8; 32],
    ) -> Result<(), CryptoError> {
        software_hmac_sha256(key, parts, output)
    }
}

#[cfg(target_arch = "riscv32")]
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

    let parts = [&b"Hi There"[..]];
    let mut sha1 = [0; 20];
    Ws63CryptoProvider.hmac_sha1(&[0x0b; 20], &parts, &mut sha1)?;
    if sha1 != HMAC_SHA1_EXPECTED {
        return Err(CryptoError(0xffff_0101));
    }
    let mut sha256 = [0; 32];
    Ws63CryptoProvider.hmac_sha256(&[0x0b; 20], &parts, &mut sha256)?;
    if sha256 != HMAC_SHA256_EXPECTED {
        return Err(CryptoError(0xffff_0102));
    }

    let encrypt = unsafe { aes_encrypt_init(KEY.as_ptr(), KEY.len()) };
    if encrypt.is_null() {
        return Err(CryptoError(0xffff_0201));
    }
    let mut cipher = [0; 16];
    let result = unsafe { aes_encrypt(encrypt, PLAIN.as_ptr(), cipher.as_mut_ptr()) };
    unsafe { aes_encrypt_deinit(encrypt) };
    if result != 0 || cipher != CIPHER {
        return Err(CryptoError(0xffff_0202));
    }

    let decrypt = unsafe { aes_decrypt_init(KEY.as_ptr(), KEY.len()) };
    if decrypt.is_null() {
        return Err(CryptoError(0xffff_0203));
    }
    let mut plain = [0; 16];
    let result = unsafe { aes_decrypt(decrypt, CIPHER.as_ptr(), plain.as_mut_ptr()) };
    unsafe { aes_decrypt_deinit(decrypt) };
    if result != 0 || plain != PLAIN {
        return Err(CryptoError(0xffff_0204));
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
    use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit, generic_array::GenericArray};

    if context.is_null() || input.is_null() || output.is_null() {
        return -1;
    }
    let context = unsafe { &*context.cast::<AesContext>() };
    let mut block =
        GenericArray::clone_from_slice(unsafe { core::slice::from_raw_parts(input, 16) });
    let result = match context.key_len {
        16 => aes::Aes128::new_from_slice(&context.key[..16]).map(|cipher| {
            if decrypt {
                cipher.decrypt_block(&mut block);
            } else {
                cipher.encrypt_block(&mut block);
            }
        }),
        24 => aes::Aes192::new_from_slice(&context.key[..24]).map(|cipher| {
            if decrypt {
                cipher.decrypt_block(&mut block);
            } else {
                cipher.encrypt_block(&mut block);
            }
        }),
        32 => aes::Aes256::new_from_slice(&context.key).map(|cipher| {
            if decrypt {
                cipher.decrypt_block(&mut block);
            } else {
                cipher.encrypt_block(&mut block);
            }
        }),
        _ => return -1,
    };
    if result.is_err() {
        return -1;
    }
    unsafe { core::ptr::copy_nonoverlapping(block.as_ptr(), output, 16) };
    0
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
const HMAC_SHA1: u32 = 0x10f6_90a0;

#[cfg(target_arch = "riscv32")]
#[repr(C)]
struct Pbkdf2Parameters {
    hash_type: u32,
    password: *mut u8,
    password_len: u32,
    salt: *mut u8,
    salt_len: u32,
    iterations: u16,
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
    operation: impl FnOnce(&[&[u8]], &mut [u8; N]),
) -> i32 {
    if output.is_null() {
        return -1;
    }
    let Some(parts) = (unsafe { with_ffi_parts::<8>(count, addresses, lengths) }) else {
        return -1;
    };
    let output = unsafe { &mut *output.cast::<[u8; N]>() };
    operation(&parts[..count], output);
    0
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
        |key, parts, output| Ws63CryptoProvider.hmac_sha1(key, parts, output),
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
        |key, parts, output| Ws63CryptoProvider.hmac_sha256(key, parts, output),
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
    ffi_digest::<20>(count, addresses, lengths, output, software_sha1)
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
extern "C" fn sha256_vector(
    count: usize,
    addresses: *const *const u8,
    lengths: *const usize,
    output: *mut u8,
) -> i32 {
    ffi_digest::<32>(count, addresses, lengths, output, software_sha256)
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
    Ws63CryptoProvider
        .pbkdf2_hmac_sha1(password, salt, iterations as u16, output)
        .map(|()| 0)
        .unwrap_or(-1)
}

#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
extern "C" fn crypto_get_random(output: *mut core::ffi::c_void, length: usize) -> i32 {
    if length != 0 && output.is_null() {
        return -1;
    }
    let Ok(length) = u32::try_from(length) else {
        return -1;
    };
    let result = unsafe { uapi_drv_cipher_trng_get_random_bytes(output.cast(), length) };
    if result == 0 { 0 } else { -1 }
}

#[cfg(target_arch = "riscv32")]
unsafe extern "C" {
    fn uapi_drv_cipher_pbkdf2(
        parameters: *const Pbkdf2Parameters,
        output: *mut u8,
        output_len: u32,
    ) -> u32;
    fn uapi_drv_cipher_trng_get_random_bytes(output: *mut u8, length: u32) -> u32;
    fn os_zalloc(size: usize) -> *mut core::ffi::c_void;
    fn os_free(pointer: *mut core::ffi::c_void);
}

#[cfg(target_arch = "riscv32")]
const _: () = {
    assert!(core::mem::size_of::<Pbkdf2Parameters>() == 24);
};

#[cfg(all(test, feature = "wifi-security-rustcrypto"))]
mod tests {
    use super::{CryptoProvider, RustCryptoProvider};

    #[test]
    fn derives_ieee_wpa_pmk() {
        let mut output = [0; 32];
        RustCryptoProvider
            .pbkdf2_hmac_sha1(b"password", b"IEEE", 4096, &mut output)
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
            .pbkdf2_hmac_sha1(b"password", b"salt", 1, &mut output)
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
        RustCryptoProvider
            .hmac_sha1(&[0x0b; 20], &parts, &mut sha1)
            .unwrap();
        RustCryptoProvider
            .hmac_sha256(&[0x0b; 20], &parts, &mut sha256)
            .unwrap();
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
