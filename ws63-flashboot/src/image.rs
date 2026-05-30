//! Image header validation for WS63 flashboot.
//!
//! Validates the application image header (key_area + code_info).

use crate::sfc::ImageHeader;

/// Validate an image header.
///
/// Checks:
/// 1. Image ID is non-zero (0xFFFFFFFF = erased flash = no image)
/// 2. Structure length is within expected bounds
/// 3. Image length is reasonable (non-zero, < 8MB)
/// 4. Signature length is within expected bounds
pub fn validate_header(header: &ImageHeader) -> bool {
    // Check image_id: 0xFFFFFFFF means erased/empty flash
    if header.code_info.image_id == 0xFFFF_FFFF || header.code_info.image_id == 0 {
        return false;
    }

    // Check structure length: should be 0x200 (ecc/sm2) or 0x400 (rsa3072)
    let struct_len = header.code_info.structure_length;
    if struct_len != 0x200 && struct_len != 0x400 {
        return false;
    }

    // Check image length: must be non-zero and reasonable (< 8MB)
    let img_len = header.code_info.image_length;
    if img_len == 0 || img_len > 8 * 1024 * 1024 {
        return false;
    }

    // Check signature length: should match key type
    let sig_len = header.code_info.signature_length;
    if sig_len == 0 || sig_len > 512 {
        return false;
    }

    // Check structure version (should be 0x0001_0000)
    if header.code_info.structure_version != 0x0001_0000 {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sfc::{CodeInfo, KeyArea};

    #[test]
    fn test_valid_header() {
        let h = ImageHeader {
            key_area: KeyArea {
                key_id: 1, key_type: 1, key_length: 32,
                sig_length: 64, sig_scheme: 1,
                _reserved: [0u8; 0xF0],
            },
            code_info: CodeInfo {
                image_id: 0x0000_0001,
                structure_version: 0x0001_0000,
                structure_length: 0x200,
                signature_length: 64,
                image_version: 1,
                image_length: 0x0010_0000, // 1MB
                load_addr: 0x2303_0000,
                _reserved: [0u8; 0x1E4],
            },
        };
        assert!(validate_header(&h));
    }

    #[test]
    fn test_erased_flash() {
        let mut h = ImageHeader {
            key_area: KeyArea {
                key_id: 0, key_type: 0, key_length: 0,
                sig_length: 0, sig_scheme: 0,
                _reserved: [0u8; 0xF0],
            },
            code_info: CodeInfo {
                image_id: 0xFFFF_FFFF, // erased flash
                structure_version: 0,
                structure_length: 0,
                signature_length: 0,
                image_version: 0,
                image_length: 0,
                load_addr: 0,
                _reserved: [0u8; 0x1E4],
            },
        };
        assert!(!validate_header(&h));
    }

    #[test]
    fn test_zero_image_id() {
        let mut h = ImageHeader {
            key_area: KeyArea {
                key_id: 0, key_type: 0, key_length: 0,
                sig_length: 0, sig_scheme: 0,
                _reserved: [0u8; 0xF0],
            },
            code_info: CodeInfo {
                image_id: 0, // invalid
                structure_version: 0x0001_0000,
                structure_length: 0x200,
                signature_length: 64,
                image_version: 1,
                image_length: 0x0010_0000,
                load_addr: 0,
                _reserved: [0u8; 0x1E4],
            },
        };
        assert!(!validate_header(&h));
    }

    #[test]
    fn test_invalid_structure_length() {
        let mut h = ImageHeader {
            key_area: KeyArea {
                key_id: 1, key_type: 1, key_length: 32,
                sig_length: 64, sig_scheme: 1,
                _reserved: [0u8; 0xF0],
            },
            code_info: CodeInfo {
                image_id: 0x0000_0001,
                structure_version: 0x0001_0000,
                structure_length: 0x100, // invalid (should be 0x200 or 0x400)
                signature_length: 64,
                image_version: 1,
                image_length: 0x0010_0000,
                load_addr: 0,
                _reserved: [0u8; 0x1E4],
            },
        };
        assert!(!validate_header(&h));
    }
}
