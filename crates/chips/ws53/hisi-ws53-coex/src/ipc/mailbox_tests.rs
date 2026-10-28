use super::*;
use crate::{MessageId, Priority};

const ROUTE: Route = Route::ApplicationToControl(Channel::Channel0);

fn size(capacity: usize) -> MailboxSize {
    MailboxSize::try_from_total_bytes(capacity + METADATA).unwrap()
}
fn message(payload: &[u8]) -> Message<'_> {
    Message::new(
        MessageId::UPDATE_SHARED_RANDOM_NUMBER,
        Priority::Low,
        payload,
    )
    .unwrap()
}

#[test]
fn size_route_and_metadata_bounds_do_not_rewrite_bad_images() {
    for total in 0..16 {
        assert_eq!(MailboxSize::try_from_total_bytes(total), None);
    }
    assert_eq!(size(65536).capacity(), 65536);
    for total in [65549, 65550, usize::MAX] {
        assert_eq!(MailboxSize::try_from_total_bytes(total), None);
    }
    let mut bytes = [0xa5; 76];
    let before = bytes;
    assert!(matches!(
        MailboxImage::initialize(&mut bytes, size(65), ROUTE),
        Err(MailboxError::StorageSize)
    ));
    assert_eq!(bytes, before);
    for route in [
        ROUTE,
        Route::ApplicationToControl(Channel::Channel1),
        Route::ControlToApplication(Channel::Channel0),
        Route::ControlToApplication(Channel::Channel1),
    ] {
        let image = MailboxImage::initialize(&mut bytes, size(64), route).unwrap();
        assert_eq!(&image.bytes()[..3], &route.header());
        assert_eq!(&image.bytes()[3..12], &[1, 0, 0, 0, 0, 64, 0, 0, 0]);
        assert_eq!(&image.bytes()[12..], &[0xa5; 64]);
    }
    MailboxImage::initialize(&mut bytes, size(64), ROUTE).unwrap();
    for (offset, value, error) in [
        (0, 0, MailboxError::Route),
        (1, 2, MailboxError::Route),
        (2, 1, MailboxError::Route),
        (3, 2, MailboxError::EmptyFlag),
        (3, 255, MailboxError::EmptyFlag),
        (4, 64, MailboxError::Cursor),
        (5, 255, MailboxError::Cursor),
        (6, 64, MailboxError::Cursor),
        (7, 255, MailboxError::Cursor),
        (8, 0, MailboxError::Capacity),
        (9, 1, MailboxError::Capacity),
        (10, 1, MailboxError::Capacity),
        (11, 1, MailboxError::Capacity),
    ] {
        let mut bad = bytes;
        bad[offset] = value;
        let before = bad;
        assert!(matches!(MailboxImage::attach(&mut bad, size(64), ROUTE), Err(e) if e == error));
        assert_eq!(bad, before);
    }
    let before = bytes;
    assert!(matches!(
        MailboxImage::attach(&mut bytes, size(65), ROUTE),
        Err(MailboxError::StorageSize)
    ));
    assert_eq!(bytes, before);
}

#[test]
fn all_messages_wrap_headers_and_payloads_without_touching_guards_or_output_tail() {
    let payload: [u8; 60] = core::array::from_fn(|i| (i * 37 + 11) as u8);
    let mut cases = 0;
    for capacity in [4, 5, 7, 64, 65, 244, 1268, 65536] {
        let mut backing = std::vec![0xa5; capacity + METADATA + 2];
        for cursor in [
            0,
            capacity / 2,
            capacity - 4,
            capacity - 3,
            capacity - 2,
            capacity - 1,
        ] {
            for id in 0..19 {
                for priority in [Priority::High, Priority::Low] {
                    for length in 0..=60 {
                        let bytes = &mut backing[1..capacity + METADATA + 1];
                        MailboxImage::initialize(bytes, size(capacity), ROUTE).unwrap();
                        bytes[4..6].copy_from_slice(&(cursor as u16).to_le_bytes());
                        bytes[6..8].copy_from_slice(&(cursor as u16).to_le_bytes());
                        let before = bytes.to_vec();
                        let mut image = MailboxImage::attach(bytes, size(capacity), ROUTE).unwrap();
                        let input = Message::new(
                            MessageId::try_from_raw(id).unwrap(),
                            priority,
                            &payload[..length],
                        )
                        .unwrap();
                        if length + 4 > capacity {
                            assert_eq!(image.try_push(input), Err(MailboxError::Full));
                            assert_eq!(image.bytes(), before);
                        } else {
                            image.try_push(input).unwrap();
                            assert_eq!(image.queued_bytes(), Ok(length + 4));
                            assert_eq!(image.free_bytes(), Ok(capacity - length - 4));
                            let next = (cursor + length + 4) % capacity;
                            assert_eq!(&image.bytes()[6..8], &(next as u16).to_le_bytes());
                            let mut output = [0xa5; MAX_MESSAGE];
                            let output_message = image.try_pop_into(&mut output).unwrap().unwrap();
                            assert_eq!(output_message, input);
                            assert_eq!(&output[..4], &[id, priority as u8, length as u8, 0]);
                            assert!(output[length + 4..].iter().all(|v| *v == 0xa5));
                            assert_eq!(image.queued_bytes(), Ok(0));
                            assert_eq!(image.free_bytes(), Ok(capacity));
                            assert_eq!(
                                &image.bytes()[4..8],
                                &[next as u8, (next >> 8) as u8, next as u8, (next >> 8) as u8]
                            );
                            assert_eq!(image.bytes()[3], 1);
                            let old = output;
                            assert_eq!(image.try_pop_into(&mut output), Ok(None));
                            assert_eq!(output, old);
                        }
                        assert_eq!([backing[0], backing[capacity + METADATA + 1]], [0xa5; 2]);
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 111264);
}

#[test]
fn full_empty_and_differing_cursor_flag_follow_serialized_sdk_semantics() {
    let mut bytes = [0; 76];
    let mut image = MailboxImage::initialize(&mut bytes, size(64), ROUTE).unwrap();
    image.try_push(message(&[0x33; 60])).unwrap();
    assert_eq!(&image.bytes()[3..8], &[0, 0, 0, 0, 0]);
    assert_eq!(image.queued_bytes(), Ok(64));
    let before = image.bytes().to_vec();
    assert_eq!(image.try_push(message(&[])), Err(MailboxError::Full));
    assert_eq!(image.bytes(), before);
    let mut output = [0; MAX_MESSAGE];
    assert_eq!(
        image.try_pop_into(&mut output).unwrap().unwrap().payload(),
        &[0x33; 60]
    );
    assert_eq!(image.queued_bytes(), Ok(0));
    image.try_push(message(&[7])).unwrap();
    // ROM ignores empty when cursors differ. Accept such a serialized snapshot
    // for layout compatibility, without claiming it is safe as a racing view.
    bytes[3] = 1;
    let mut image = MailboxImage::attach(&mut bytes, size(64), ROUTE).unwrap();
    assert_eq!(image.queued_bytes(), Ok(5));
    assert_eq!(
        image.try_pop_into(&mut output).unwrap().unwrap().payload(),
        &[7]
    );
}

fn rejects_frame(frame: &[u8], published: usize, error: MailboxError) {
    let mut bytes = [0xa5; 76];
    MailboxImage::initialize(&mut bytes, size(64), ROUTE).unwrap();
    let cursor = 62;
    bytes[4..6].copy_from_slice(&(cursor as u16).to_le_bytes());
    bytes[6..8].copy_from_slice(&(((cursor + published) % 64) as u16).to_le_bytes());
    bytes[3] = 0;
    for (i, byte) in frame.iter().enumerate() {
        bytes[METADATA + (cursor + i) % 64] = *byte;
    }
    let before = bytes;
    let mut output = [0xa5; MAX_MESSAGE];
    let mut image = MailboxImage::attach(&mut bytes, size(64), ROUTE).unwrap();
    assert_eq!(image.try_pop_into(&mut output), Err(error));
    assert_eq!(image.bytes(), before);
    assert_eq!(output, [0xa5; MAX_MESSAGE]);
}

#[test]
fn malformed_messages_never_consume_or_change_output_even_across_wrap() {
    for published in 1..4 {
        rejects_frame(&[18, 1, 0, 0], published, MailboxError::Incomplete);
    }
    for length in 1..=60 {
        rejects_frame(&[18, 1, length, 0], 4, MailboxError::Incomplete);
    }
    for length in 61..=u16::MAX {
        let [lo, hi] = length.to_le_bytes();
        rejects_frame(
            &[18, 1, lo, hi],
            4,
            MailboxError::Message(MessageError::PayloadTooLong),
        );
    }
    for id in 19..=u8::MAX {
        rejects_frame(
            &[id, 1, 0, 0],
            4,
            MailboxError::Message(MessageError::UnknownId),
        );
    }
    for priority in 2..=u8::MAX {
        rejects_frame(
            &[18, priority, 0, 0],
            4,
            MailboxError::Message(MessageError::UnknownPriority),
        );
    }
}

#[test]
fn mixed_sequence_matches_an_independent_fifo_through_many_wraps() {
    use std::collections::VecDeque;
    let mut bytes = [0; 256];
    let mut image = MailboxImage::initialize(&mut bytes, size(244), ROUTE).unwrap();
    let mut expected = VecDeque::new();
    let mut queued = 0;
    let mut seed = 0x12345678u32;
    let mut successful_pushes = 0;
    let mut rejected_pushes = 0;
    let mut pops = 0;
    let mut output = [0; MAX_MESSAGE];
    for n in 0..20000 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        if seed & 3 != 0 {
            let length = ((seed >> 8) % 61) as usize;
            let payload = std::vec![(n & 255) as u8; length];
            let before = image.bytes().to_vec();
            if length + 4 <= 244 - queued {
                image.try_push(message(&payload)).unwrap();
                expected.push_back(payload);
                queued += length + 4;
                successful_pushes += 1;
            } else {
                assert_eq!(image.try_push(message(&payload)), Err(MailboxError::Full));
                assert_eq!(image.bytes(), before);
                rejected_pushes += 1;
            }
        } else if let Some(payload) = expected.pop_front() {
            assert_eq!(
                image.try_pop_into(&mut output).unwrap().unwrap().payload(),
                payload
            );
            queued -= payload.len() + 4;
            pops += 1;
        } else {
            assert_eq!(image.try_pop_into(&mut output), Ok(None));
        }
        assert_eq!(image.queued_bytes(), Ok(queued));
        assert_eq!(image.free_bytes(), Ok(244 - queued));
    }
    assert!(successful_pushes > 5000 && pops > 4900 && rejected_pushes > 4000);
}
