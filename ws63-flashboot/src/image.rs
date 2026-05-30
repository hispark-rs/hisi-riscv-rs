//! Image header validation for WS63 flashboot.
//!
//! Checks: image_id ≠ 0/0xFFFFFFFF, structure_length ∈ {0x200, 0x400},
//! image_length ∈ (0, 8MB), signature_length ∈ (0, 512], version = 0x0001_0000.

use crate::sfc::ImageHeader;

/// Validate an image header. Returns true if the image looks bootable.
pub fn validate(header: &ImageHeader) -> bool {
    let ci = &header.code_info;
    ci.image_id != 0
        && ci.image_id != 0xFFFF_FFFF
        && (ci.structure_length == 0x200 || ci.structure_length == 0x400)
        && ci.image_length > 0
        && ci.image_length < 8 * 1024 * 1024
        && ci.signature_length > 0
        && ci.signature_length <= 512
        && ci.structure_version == 0x0001_0000
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sfc::{CodeInfo, ImageHeader, KeyArea};

    fn make_header() -> ImageHeader {
        ImageHeader {
            key_area: KeyArea {
                key_id: 0,
                key_type: 0,
                key_length: 0,
                sig_length: 0,
                sig_scheme: 0,
                _reserved: [0u8; 0xF0],
            },
            code_info: CodeInfo {
                image_id: 1,
                structure_version: 0x0001_0000,
                structure_length: 0x200,
                signature_length: 256,
                image_version: 1,
                image_length: 0x10000,
                load_addr: 0xA00000,
                image_hash: [0u8; 32],
                _reserved: [0u8; 0x1C4],
            },
        }
    }

    #[test]
    fn test_validate_valid_header() {
        assert!(validate(&make_header()));
    }

    #[test]
    fn test_reject_zero_image_id() {
        let mut h = make_header();
        h.code_info.image_id = 0;
        assert!(!validate(&h));
    }

    #[test]
    fn test_reject_ffffffff_image_id() {
        let mut h = make_header();
        h.code_info.image_id = 0xFFFF_FFFF;
        assert!(!validate(&h));
    }

    #[test]
    fn test_reject_zero_image_length() {
        let mut h = make_header();
        h.code_info.image_length = 0;
        assert!(!validate(&h));
    }

    #[test]
    fn test_reject_too_large_image() {
        let mut h = make_header();
        h.code_info.image_length = 8 * 1024 * 1024; // exactly 8MB — must be LESS than
        assert!(!validate(&h));
    }

    #[test]
    fn test_accept_max_valid_image_length() {
        let mut h = make_header();
        h.code_info.image_length = 8 * 1024 * 1024 - 1; // 1 byte under 8MB
        assert!(validate(&h));
    }

    #[test]
    fn test_reject_zero_signature_length() {
        let mut h = make_header();
        h.code_info.signature_length = 0;
        assert!(!validate(&h));
    }

    #[test]
    fn test_reject_excessive_signature_length() {
        let mut h = make_header();
        h.code_info.signature_length = 513;
        assert!(!validate(&h));
    }

    #[test]
    fn test_accept_512_signature_length() {
        let mut h = make_header();
        h.code_info.signature_length = 512;
        assert!(validate(&h));
    }

    #[test]
    fn test_reject_wrong_structure_version() {
        let mut h = make_header();
        h.code_info.structure_version = 0x0000_0001;
        assert!(!validate(&h));
    }

    #[test]
    fn test_accept_valid_structure_lengths() {
        let mut h = make_header();
        h.code_info.structure_length = 0x200;
        assert!(validate(&h));
        h.code_info.structure_length = 0x400;
        assert!(validate(&h));
    }

    #[test]
    fn test_reject_invalid_structure_length() {
        let mut h = make_header();
        h.code_info.structure_length = 0x300;
        assert!(!validate(&h));
    }
}