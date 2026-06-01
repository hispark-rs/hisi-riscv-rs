//! Image header validation for WS63 flashboot.
//!
//! Best-effort **structural sanity** check of the `image_code_info_t` header
//! (NOT a signature/authenticity check — see the crate-level docs): image_id is
//! set, the structure version/length and signature length look sane, and the
//! signed body length (`code_area_len`) is in range.

use crate::sfc::ImageHeader;

/// Validate an image header structurally. Returns true if it looks bootable.
///
/// Integrity/sanity gate only — it does NOT authenticate the image (the vendor
/// flashboot ECC/SM2-verifies it against an efuse-rooted key; this loader does not).
pub fn validate(header: &ImageHeader) -> bool {
    let ci = &header.code_info;
    ci.image_id != 0
        && ci.image_id != 0xFFFF_FFFF
        && ci.structure_version == 0x0001_0000
        // 0x200 = ECC256/SM2 CodeInfo; 0x400 = RSA3072 CodeInfo.
        && (ci.structure_length == 0x200 || ci.structure_length == 0x400)
        && ci.signature_length > 0
        && ci.signature_length <= 512
        && ci.code_area_len > 0
        && ci.code_area_len < 8 * 1024 * 1024
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_header() -> ImageHeader {
        // SAFETY: ImageHeader is repr(C) over plain u32 / byte arrays, so the all-zero
        // bit pattern is valid (just not bootable). We then set the fields validate() reads.
        let mut h: ImageHeader = unsafe { core::mem::zeroed() };
        h.code_info.image_id = 1;
        h.code_info.structure_version = 0x0001_0000;
        h.code_info.structure_length = 0x200;
        h.code_info.signature_length = 64; // ECC256
        h.code_info.code_area_len = 0x10000;
        h
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
    fn test_reject_zero_code_area_len() {
        let mut h = make_header();
        h.code_info.code_area_len = 0;
        assert!(!validate(&h));
    }

    #[test]
    fn test_reject_too_large_image() {
        let mut h = make_header();
        h.code_info.code_area_len = 8 * 1024 * 1024; // exactly 8MB — must be LESS than
        assert!(!validate(&h));
    }

    #[test]
    fn test_accept_max_valid_code_area_len() {
        let mut h = make_header();
        h.code_info.code_area_len = 8 * 1024 * 1024 - 1; // 1 byte under 8MB
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
