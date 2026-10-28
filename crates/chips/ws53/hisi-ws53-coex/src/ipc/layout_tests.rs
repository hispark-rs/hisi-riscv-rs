use super::*;
use crate::mailbox::MailboxImage;

fn header(base: u32) -> [u8; 28] {
    let mut header = [0xa5; 28];
    header[24..28].copy_from_slice(&base.to_le_bytes());
    header
}

#[test]
fn observed_factory_queues_use_control_local_alias_and_sdk_allocation_order() {
    let sram = ControlSramRange::try_from_bounds(0x10000, 0x1ff00).unwrap();
    let layout = SdkMailboxLayout::from_boot_header(&header(0x1e300), sram).unwrap();
    // Public queue metadata from the 2026-10-27 private, read-only capture.
    // No payload, vendor instructions, pointers or live memory references.
    let observations = [
        (
            Route::ControlToApplication(Channel::Channel0),
            0x1e300,
            0x500,
            1236u16,
        ),
        (
            Route::ControlToApplication(Channel::Channel1),
            0x1e800,
            0x100,
            24,
        ),
        (
            Route::ApplicationToControl(Channel::Channel0),
            0x1e900,
            0x500,
            968,
        ),
        (
            Route::ApplicationToControl(Channel::Channel1),
            0x1ee00,
            0x100,
            12,
        ),
    ];
    for (index, &(route, local, total, cursor)) in observations.iter().enumerate() {
        let placement = *layout.for_route(route);
        assert_eq!(placement, layout.placements()[index]);
        assert_eq!(placement.route(), route);
        assert_eq!(placement.control_address().raw(), local);
        assert_eq!(
            placement.control_address().application_view().raw(),
            local + 0x1000_0000
        );
        assert_eq!(placement.size().total_bytes(), total);
        let identity = match route {
            Route::ControlToApplication(c) => [0, 2, c as u8],
            Route::ApplicationToControl(c) => [2, 0, c as u8],
        };
        let mut bytes = std::vec![0; total];
        bytes[..3].copy_from_slice(&identity);
        bytes[3] = 1;
        bytes[4..6].copy_from_slice(&cursor.to_le_bytes());
        bytes[6..8].copy_from_slice(&cursor.to_le_bytes());
        bytes[8..12].copy_from_slice(&(u32::try_from(total - 12).unwrap()).to_le_bytes());
        let before = bytes.clone();
        let mut image = MailboxImage::attach(&mut bytes, placement.size(), route).unwrap();
        assert!(image.try_pop_into(&mut [0u8; 64]).unwrap().is_none());
        assert_eq!(image.bytes(), before);
    }
}

#[test]
fn explicit_sram_extent_alignment_alias_and_boot_reservation_are_not_clamped() {
    for (start, end) in [
        (0, 0x1ff00),
        (0x10000, 0x24000),
        (0x10000, 0x10000),
        (0x20000, 0x10000),
        (0x10001, 0x1ff00),
        (0x10000, 0x1ff01),
    ] {
        assert_eq!(ControlSramRange::try_from_bounds(start, end), None);
    }
    let sram = ControlSramRange::try_from_bounds(0x10000, 0x1ff00).unwrap();
    for base in [0x10080, 0x1e300, 0x1f300] {
        assert!(SdkMailboxLayout::from_boot_header(&header(base), sram).is_ok());
    }
    for base in [0x1e301, 0x1e302, 0x1e303] {
        assert_eq!(
            SdkMailboxLayout::from_boot_header(&header(base), sram),
            Err(LayoutError::Alignment)
        );
    }
    for base in [0, 0xfffc, 0x1f304, 0x1001e300, u32::MAX - 3] {
        assert_eq!(
            SdkMailboxLayout::from_boot_header(&header(base), sram),
            Err(LayoutError::OutsideSram)
        );
    }
    for base in [0x10000, 0x1007c] {
        assert_eq!(
            SdkMailboxLayout::from_boot_header(&header(base), sram),
            Err(LayoutError::BootHeaderOverlap)
        );
    }
    let narrow = ControlSramRange::try_from_bounds(0x1e300, 0x1ef00).unwrap();
    assert!(SdkMailboxLayout::from_boot_header(&header(0x1e300), narrow).is_ok());
    assert_eq!(
        SdkMailboxLayout::from_boot_header(&header(0x1e2fc), narrow),
        Err(LayoutError::OutsideSram)
    );
    let device = ControlSramRange::try_from_bounds(0x10000, 0x23f00).unwrap();
    assert!(SdkMailboxLayout::from_boot_header(&header(0x23300), device).is_ok());
}

#[test]
fn layout_debug_types_support_defmt_without_acquiring_a_peer_lease() {
    #[cfg(feature = "defmt")]
    fn require_format<T: defmt::Format>() {}
    #[cfg(feature = "defmt")]
    {
        require_format::<ControlLocalAddress>();
        require_format::<ApplicationViewAddress>();
        require_format::<ControlSramRange>();
        require_format::<LayoutError>();
        require_format::<MailboxPlacement>();
        require_format::<SdkMailboxLayout>();
    }
    assert_ne!(
        core::any::TypeId::of::<ControlLocalAddress>(),
        core::any::TypeId::of::<ApplicationViewAddress>()
    );
}
