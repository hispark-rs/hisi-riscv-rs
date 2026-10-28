use super::*;
use crate::{MAX_PAYLOAD, Priority};

fn payload(kind: u32, start: u32, length: u32) -> [u8; 12] {
    let mut bytes = [0; 12];
    for (out, value) in bytes
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip([kind, start, length])
    {
        out.copy_from_slice(&value.to_le_bytes());
    }
    bytes
}
fn message(bytes: &[u8]) -> Message<'_> {
    Message::new(MessageId::UPDATE_SHARED_RANDOM_NUMBER, Priority::Low, bytes).unwrap()
}

#[test]
fn explicit_bounds_reject_wrap_and_do_not_assume_ct_sram() {
    for (start, end) in [(0, 0), (4, 3), (0, u32::MAX), (0xefffffff, 0xf0000000)] {
        assert!(BufferRegion::try_from_control_bounds(start, end).is_none());
    }
    for (start, end) in [
        (0, 1),
        (0x1a000, 0x1a080),
        (0xc1000, 0xc1080),
        (0xeffffffe, 0xefffffff),
    ] {
        let r = BufferRegion::try_from_control_bounds(start, end).unwrap();
        assert_eq!(
            (r.control_start(), r.application_start(), r.byte_len()),
            (start, start + 0x10000000, end - start)
        );
    }
}

#[test]
fn fixed_frame_id_type_endian_and_extent_rejections() {
    let r = BufferRegion::try_from_control_bounds(0xc1000, 0xc1080).unwrap();
    let bytes = payload(0, 0xc1000, 128);
    for priority in [Priority::High, Priority::Low] {
        let valid = Message::new(MessageId::UPDATE_SHARED_RANDOM_NUMBER, priority, &bytes).unwrap();
        assert_eq!(UpdateRequest::decode(valid, r).unwrap().byte_len(), 128);
    }
    for id in 0..18 {
        let m = Message::new(MessageId::try_from_raw(id).unwrap(), Priority::Low, &bytes).unwrap();
        assert_eq!(UpdateRequest::decode(m, r), Err(RequestError::WrongMessage));
    }
    for n in 0..=MAX_PAYLOAD {
        if n != 12 {
            assert_eq!(
                UpdateRequest::decode(message(&[0; MAX_PAYLOAD][..n]), r),
                Err(RequestError::PayloadLength)
            );
        }
    }
    for raw in [3, 255, 0x100, 0x01000000, u32::MAX] {
        assert_eq!(
            UpdateRequest::decode(message(&payload(raw, 0xc1000, 128)), r),
            Err(RequestError::UnknownUpdate)
        );
    }
    for (start, len, error) in [
        (0xc1000, 0, RequestError::InvalidExtent),
        (u32::MAX - 3, 8, RequestError::InvalidExtent),
        (0xeffffff0, 16, RequestError::InvalidExtent),
        (0x100c1000, 128, RequestError::UnregisteredRegion),
        (0xc1000, 64, RequestError::UnregisteredRegion),
        (0xc1001, 128, RequestError::UnregisteredRegion),
        (0xc0fff, 130, RequestError::UnregisteredRegion),
    ] {
        assert_eq!(
            UpdateRequest::decode(message(&payload(0, start, len)), r),
            Err(error)
        );
    }
}

#[test]
fn arbitrary_tails_and_equal_halves_are_explicit_without_clamping() {
    for length in 1..=129u32 {
        let region = BufferRegion::try_from_control_bounds(0xc1000, 0xc1000 + length).unwrap();
        for kind in 0..3 {
            let bytes = payload(kind, 0xc1000, length);
            let result = UpdateRequest::decode(message(&bytes), region);
            if kind != 0 && !length.is_multiple_of(2) {
                assert_eq!(result, Err(RequestError::UnevenHalves));
                continue;
            }
            let r = result.unwrap();
            let n = if kind == 0 { length } else { length / 2 } as usize;
            let offset = if kind == 2 { n } else { 0 };
            assert_eq!((r.byte_len(), r.offset()), (n, offset));
            let mut guarded = [0xa5; 131];
            let mut scratch = [0x33; 132];
            let mut image =
                RandomBufferImage::attach(region, &mut guarded[1..1 + length as usize]).unwrap();
            assert_eq!(image.check::<()>(r, n), Ok(()));
            image
                .stage_and_copy(r, &mut scratch, |out| {
                    for (i, b) in out.iter_mut().enumerate() {
                        *b = i as u8;
                    }
                    Ok::<(), ()>(())
                })
                .unwrap();
            for (i, b) in image.bytes().iter().enumerate() {
                assert_eq!(
                    *b,
                    if (offset..offset + n).contains(&i) {
                        (i - offset) as u8
                    } else {
                        0xa5
                    }
                );
            }
            assert!(scratch[n..].iter().all(|b| *b == 0x33));
            assert_eq!(guarded[0], 0xa5);
            assert!(guarded[1 + length as usize..].iter().all(|b| *b == 0xa5));
        }
    }
}

#[test]
fn attach_mismatch_and_cross_registration_do_not_mutate() {
    let region = BufferRegion::try_from_control_bounds(0xc1000, 0xc1080).unwrap();
    let mut bytes = [0xa5; 129];
    assert!(RandomBufferImage::attach(region, &mut bytes).is_none());
    let other = BufferRegion::try_from_control_bounds(0xc2000, 0xc2080).unwrap();
    let request = UpdateRequest::decode(message(&payload(0, 0xc2000, 128)), other).unwrap();
    let image = RandomBufferImage::attach(region, &mut bytes[..128]).unwrap();
    assert_eq!(
        image.check::<()>(request, 128),
        Err(ServiceError::Request(RequestError::UnregisteredRegion))
    );
    let request = UpdateRequest::decode(message(&payload(0, 0xc1000, 128)), region).unwrap();
    for n in 0..128 {
        assert_eq!(
            image.check::<()>(request, n),
            Err(ServiceError::ScratchTooSmall)
        );
    }
    assert_eq!(bytes, [0xa5; 129]);
}

#[test]
fn public_entropy_callback_preserves_destination_on_failure() {
    let region = BufferRegion::try_from_control_bounds(0xc1000, 0xc1080).unwrap();
    let request = UpdateRequest::decode(message(&payload(0, 0xc1000, 128)), region).unwrap();
    let mut bytes = [0xa5; 128];
    let mut scratch = [0; 128];
    let mut image = RandomBufferImage::attach(region, &mut bytes).unwrap();
    let result = image.update(request, &mut scratch, |output| {
        output[..16].fill(0x42);
        Err::<(), _>(7u8)
    });
    assert_eq!(result, Err(ServiceError::Random(7)));
    assert_eq!(image.bytes(), &[0xa5; 128]);
    image
        .update(request, &mut scratch, |output| {
            output.fill(0x5a);
            Ok::<(), u8>(())
        })
        .unwrap();
    assert_eq!(image.bytes(), &[0x5a; 128]);
}

#[test]
fn every_partial_staging_failure_leaves_destination_unchanged() {
    let region = BufferRegion::try_from_control_bounds(0xc1000, 0xc1080).unwrap();
    for kind in 0..3 {
        let request = UpdateRequest::decode(message(&payload(kind, 0xc1000, 128)), region).unwrap();
        for written in 0..=request.byte_len() {
            let mut bytes = [0xa5; 128];
            let mut scratch = [0x33; 129];
            let mut image = RandomBufferImage::attach(region, &mut bytes).unwrap();
            assert_eq!(image.check::<()>(request, scratch.len()), Ok(()));
            assert_eq!(
                image.stage_and_copy(request, &mut scratch, |out| {
                    out[..written].fill(0x42);
                    Err(written)
                }),
                Err(written)
            );
            assert_eq!(image.bytes(), &[0xa5; 128]);
            assert!(scratch[..written].iter().all(|b| *b == 0x42));
            assert!(scratch[written..].iter().all(|b| *b == 0x33));
        }
    }
}

#[cfg(feature = "defmt")]
#[test]
fn service_public_formats_have_defmt_support() {
    fn supported<T: defmt::Format>() {}
    supported::<BufferRegion>();
    supported::<UpdateKind>();
    supported::<RequestError>();
    supported::<UpdateRequest>();
    supported::<ServiceError<u8>>();
}
