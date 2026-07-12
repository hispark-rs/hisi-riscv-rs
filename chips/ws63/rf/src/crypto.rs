//! Internal Wi-Fi security provider boundary.
//!
//! The C supplicant must not make mbedTLS contexts part of the Rust API. WS63
//! uses the published unified-cipher UAPI; RustCrypto is the portable provider
//! and host-side known-answer oracle.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CryptoError(pub(crate) u32);

pub(crate) trait CryptoProvider {
    fn pbkdf2_hmac_sha1(
        &self,
        password: &[u8],
        salt: &[u8],
        iterations: u16,
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
unsafe extern "C" {
    fn uapi_drv_cipher_pbkdf2(
        parameters: *const Pbkdf2Parameters,
        output: *mut u8,
        output_len: u32,
    ) -> u32;
}

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
}
