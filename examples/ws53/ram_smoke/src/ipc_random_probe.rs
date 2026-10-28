//! Controlled RAM experiment, ordinary private images, not live peer buffers.
//! Uses the inherited TRNG handoff only after both vendor CPU clients stop.
use core::num::NonZeroU16;
use embedded_hal::delay::DelayNs;
use hisi_hal::{
    ipc::{
        Message, MessageId, Priority,
        random_buffer::{
            BufferRegion, RandomBufferImage, RequestError, ServiceError, UpdateRequest,
        },
    },
    trng::{Error, TrngDriver},
};

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Record {
    // kind,length,status,offset,selected length,staged prefix,attempts,capacity
    words: [u32; 8],
    before: [u8; 128],
    after: [u8; 128],
    scratch: [u8; 128],
}
#[repr(C)]
pub struct Report {
    // magic,version,completed,all checked,reserved[4]
    header: [u32; 8],
    records: [Record; 13],
}
#[unsafe(no_mangle)]
pub static mut WS53_IPC_RANDOM_RAM: Report = Report {
    header: [0; 8],
    records: [Record {
        words: [0; 8],
        before: [0; 128],
        after: [0; 128],
        scratch: [0; 128],
    }; 13],
};
fn code(error: RequestError) -> u32 {
    match error {
        RequestError::WrongMessage => 10,
        RequestError::PayloadLength => 11,
        RequestError::UnknownUpdate => 12,
        RequestError::InvalidExtent => 13,
        RequestError::UnregisteredRegion => 14,
        RequestError::UnevenHalves => 15,
    }
}
pub fn check(source: &mut TrngDriver<'_>, delay: &mut impl DelayNs) {
    // kind,length,wire base,payload length,staging capacity,total attempts,status
    let cases: [[u32; 7]; 13] = [
        [0, 128, 0xc1000, 12, 128, 10000, 0],
        [1, 128, 0xc1000, 12, 128, 10000, 0],
        [2, 128, 0xc1000, 12, 128, 10000, 0],
        [0, 1, 0xc1000, 12, 128, 10000, 0],
        [0, 3, 0xc1000, 12, 128, 10000, 0],
        [0, 127, 0xc1000, 12, 128, 10000, 0],
        [2, 6, 0xc1000, 12, 128, 10000, 0],
        [3, 128, 0xc1000, 12, 128, 10000, 12],
        [0, 128, 0xc1001, 12, 128, 10000, 14],
        [2, 127, 0xc1000, 12, 128, 10000, 15],
        [0, 128, 0xc1000, 11, 128, 10000, 11],
        [0, 128, 0xc1000, 12, 127, 10000, 20],
        [0, 128, 0xc1000, 12, 128, 1, 31],
    ];
    for (
        i,
        [
            kind,
            length,
            wire_base,
            payload_len,
            capacity,
            attempts,
            expected,
        ],
    ) in cases.into_iter().enumerate()
    {
        let mut data = [0xa5; 128];
        let before = data;
        let mut scratch = [0x33; 128];
        let mut words = [kind, length, 0, 0, 0, 0, attempts, capacity];
        let mut payload = [0; 12];
        for (out, value) in payload
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip([kind, wire_base, length])
        {
            out.copy_from_slice(&value.to_le_bytes());
        }
        let region = BufferRegion::try_from_control_bounds(0xc1000, 0xc1000 + length).unwrap();
        let message = Message::new(
            MessageId::UPDATE_SHARED_RANDOM_NUMBER,
            Priority::Low,
            &payload[..payload_len as usize],
        )
        .unwrap();
        let status = match UpdateRequest::decode(message, region) {
            Err(error) => code(error),
            Ok(request) => {
                words[3] = request.offset() as u32;
                words[4] = request.byte_len() as u32;
                let mut image =
                    RandomBufferImage::attach(region, &mut data[..length as usize]).unwrap();
                match image.update(
                    request,
                    source,
                    &mut scratch[..capacity as usize],
                    NonZeroU16::new(attempts as u16).unwrap(),
                    delay,
                ) {
                    Ok(()) => 0,
                    Err(ServiceError::Request(error)) => code(error),
                    Err(ServiceError::ScratchTooSmall) => 20,
                    Err(ServiceError::Random(error)) => {
                        words[5] = error.written() as u32;
                        assert_eq!(error.error(), Error::Timeout);
                        31
                    }
                }
            }
        };
        assert_eq!(status, expected);
        words[2] = status;
        if status == 0 {
            let offset = words[3] as usize;
            let n = words[4] as usize;
            assert_eq!(&data[offset..offset + n], &scratch[..n]);
            assert_eq!(&data[..offset], &before[..offset]);
            assert_eq!(&data[offset + n..], &before[offset + n..]);
            assert_eq!(&scratch[n..], &[0x33; 128][n..]);
        } else {
            assert_eq!(data, before);
            assert!(words[5] == 0 || (i == 12 && words[5] == 4));
            assert_eq!(
                &scratch[words[5] as usize..],
                &[0x33; 128][words[5] as usize..]
            );
        }
        let record = Record {
            words,
            before,
            after: data,
            scratch,
        };
        unsafe {
            // Exclusive ordinary RAM; the observer reads only after the fixture
            // stops. No numeric pointer or received address is dereferenced.
            core::ptr::addr_of_mut!(WS53_IPC_RANDOM_RAM.records)
                .cast::<Record>()
                .add(i)
                .write_volatile(record);
            core::ptr::addr_of_mut!(WS53_IPC_RANDOM_RAM.header)
                .cast::<u32>()
                .add(2)
                .write_volatile((i + 1) as u32);
        }
    }
    unsafe {
        core::arch::asm!("fence rw,rw", options(nostack));
        core::ptr::addr_of_mut!(WS53_IPC_RANDOM_RAM.header)
            .write_volatile([0x524e4432, 1, 13, 1, 0, 0, 0, 0]);
    }
}
