use super::*;

#[test]
fn sdk_discriminator_range_and_public_formats_are_exact() {
    #[cfg(feature = "defmt")]
    {
        fn format<T: defmt::Format>() {}
        format::<Channel>();
        format::<MessageId>();
        format::<Priority>();
        format::<MessageError>();
        format::<Message<'static>>();
        format::<mailbox::MailboxSize>();
        format::<mailbox::Route>();
        format::<mailbox::MailboxError>();
        format::<spsc::Snapshot>();
        format::<spsc::Error>();
    }
    assert_eq!(MessageId::UPDATE_SHARED_RANDOM_NUMBER.raw(), 18);
    for raw in 0..=u8::MAX {
        assert_eq!(MessageId::try_from_raw(raw).is_some(), raw < 19);
    }
}

#[test]
fn all_sdk_ids_priorities_and_payload_lengths_encode_exactly_and_preserve_tail() {
    let payload: [u8; MAX_PAYLOAD] = core::array::from_fn(|i| (i * 37 + 11) as u8);
    let mut cases = 0;
    for raw in 0..19 {
        for priority in [Priority::High, Priority::Low] {
            for length in 0..=MAX_PAYLOAD {
                let id = MessageId::try_from_raw(raw).unwrap();
                let message = Message::new(id, priority, &payload[..length]).unwrap();
                let mut output = [0xa5; MAX_MESSAGE];
                let used = message.encode(&mut output);
                assert_eq!(used, length + 4);
                assert_eq!(&output[..4], &[raw, priority as u8, length as u8, 0]);
                assert_eq!(&output[4..used], &payload[..length]);
                assert!(output[used..].iter().all(|n| *n == 0xa5));
                let decoded = Message::decode(&output[..used]).unwrap();
                assert_eq!(decoded.id(), id);
                assert_eq!(decoded.priority(), priority);
                assert_eq!(decoded.payload(), &payload[..length]);
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 2318);
}

#[test]
fn every_u16_oversized_length_is_rejected_before_accessing_the_payload() {
    for length in 61..=u16::MAX {
        let [low, high] = length.to_le_bytes();
        assert_eq!(
            Message::decode(&[0, 0, low, high]),
            Err(MessageError::PayloadTooLong)
        );
    }
    for length in [61, 64, 255, 256, 65535, 65536, 65540] {
        let bytes = std::vec![0; length];
        assert_eq!(
            Message::new(MessageId(0), Priority::Low, &bytes),
            Err(MessageError::PayloadTooLong)
        );
    }
}

#[test]
fn malformed_ids_priority_truncation_and_trailing_bytes_are_not_dispatched() {
    for id in 19..=u8::MAX {
        assert_eq!(
            Message::decode(&[id, 0, 0, 0]),
            Err(MessageError::UnknownId)
        );
    }
    for priority in 2..=u8::MAX {
        assert_eq!(
            Message::decode(&[18, priority, 0, 0]),
            Err(MessageError::UnknownPriority)
        );
    }
    for length in 0..4 {
        assert_eq!(
            Message::decode(&[0; 4][..length]),
            Err(MessageError::Framing)
        );
    }
    assert_eq!(Message::decode(&[18, 1, 1, 0]), Err(MessageError::Framing));
    assert_eq!(
        Message::decode(&[18, 1, 0, 0, 0xaa]),
        Err(MessageError::Framing)
    );
}
