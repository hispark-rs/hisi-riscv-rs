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