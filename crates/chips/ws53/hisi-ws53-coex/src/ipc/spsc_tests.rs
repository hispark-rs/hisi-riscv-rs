use super::*;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering as AtomicOrdering},
    },
    time::{Duration, Instant},
};

struct Memory {
    bytes: Arc<Mutex<std::vec::Vec<u8>>>,
    size: MailboxSize,
    drops: Arc<AtomicUsize>,
}
impl Drop for Memory {
    fn drop(&mut self) {
        self.drops.fetch_add(1, AtomicOrdering::SeqCst);
    }
}
fn read16(bytes: &Arc<Mutex<std::vec::Vec<u8>>>, offset: usize) -> u16 {
    let b = bytes.lock().unwrap();
    u16::from_le_bytes([b[offset], b[offset + 1]])
}
fn write16(bytes: &Arc<Mutex<std::vec::Vec<u8>>>, offset: usize, value: u16) {
    bytes.lock().unwrap()[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn read8(bytes: &Arc<Mutex<std::vec::Vec<u8>>>, offset: usize) -> u8 {
    bytes.lock().unwrap()[offset]
}
fn write8(bytes: &Arc<Mutex<std::vec::Vec<u8>>>, offset: usize, value: u8) {
    bytes.lock().unwrap()[offset] = value;
}
// SAFETY: each scalar/byte is separately locked, including atomic halfwords.
// Locks provide acquire/release. No whole transaction lock masks concurrency.
// The independent peer owns complementary fields; Arc allocation is retained.
unsafe impl SharedMailbox for Memory {
    fn size(&self) -> MailboxSize {
        self.size
    }
    fn snapshot(&self) -> Snapshot {
        let b = self.bytes.lock().unwrap();
        Snapshot {
            identity: b[..3].try_into().unwrap(),
            empty: b[3],
            read: u16::from_le_bytes(b[4..6].try_into().unwrap()),
            write: u16::from_le_bytes(b[6..8].try_into().unwrap()),
            capacity: u32::from_le_bytes(b[8..12].try_into().unwrap()),
        }
    }
    unsafe fn load_data(&self, index: usize) -> u8 {
        read8(&self.bytes, 12 + index)
    }
    unsafe fn store_data(&mut self, index: usize, value: u8) {
        write8(&self.bytes, 12 + index, value);
    }
    unsafe fn publish_write(&mut self, cursor: u16) {
        write16(&self.bytes, 6, cursor);
    }
    unsafe fn release_read(&mut self, cursor: u16) {
        write16(&self.bytes, 4, cursor);
    }
}
fn prepared(capacity: usize, tx: bool) -> Memory {
    let size = MailboxSize::try_from_total_bytes(capacity + 12).unwrap();
    let mut b = std::vec![0xa5; size.total_bytes()];
    b[..3].copy_from_slice(&identity(Channel::Channel1, tx));
    b[3] = u8::from(tx);
    b[4..6].copy_from_slice(&(if tx { 0 } else { capacity - 1 } as u16).to_le_bytes());
    b[6..8].copy_from_slice(&0u16.to_le_bytes());
    b[8..12].copy_from_slice(&(capacity as u32).to_le_bytes());
    Memory {
        bytes: Arc::new(Mutex::new(b)),
        size,
        drops: Arc::new(AtomicUsize::new(0)),
    }
}
fn wire(n: usize, length: usize) -> std::vec::Vec<u8> {
    let mut b = std::vec![(n % 19) as u8, (n % 2) as u8, length as u8, 0];
    b.extend((0..length).map(|i| (n.wrapping_mul(17).wrapping_add(i * 37)) as u8));
    b
}
fn decode(b: &[u8]) -> Message<'_> {
    Message::decode(b).unwrap()
}

// Independently transcribed original SDK consumer: cursor-first empty check,
// copies first, publishes read, compares and conditionally writes empty1.
fn sdk_pop(bytes: &Arc<Mutex<std::vec::Vec<u8>>>, capacity: usize) -> Option<std::vec::Vec<u8>> {
    let read = usize::from(read16(bytes, 4));
    let write = usize::from(read16(bytes, 6));
    if read == write && read8(bytes, 3) != 0 {
        return None;
    }
    let mut frame: std::vec::Vec<u8> = (0..4)
        .map(|i| read8(bytes, 12 + (read + i) % capacity))
        .collect();
    let length = usize::from(u16::from_le_bytes([frame[2], frame[3]]));
    assert!(length <= 60 && frame[0] < 19 && frame[1] < 2);
    for i in 0..length {
        frame.push(read8(bytes, 12 + (read + 4 + i) % capacity));
        if i % 7 == 0 {
            std::thread::yield_now();
        }
    }
    write16(bytes, 4, ((read + frame.len()) % capacity) as u16);
    let observed_write = read16(bytes, 6);
    let observed_read = read16(bytes, 4);
    if observed_read == observed_write {
        std::thread::yield_now();
        write8(bytes, 3, 1);
    }
    Some(frame)
}
// Independently transcribed original SDK producer, including exact-full and
// conditional empty0. It knows nothing about the Rust consumer's logical head.
fn sdk_push(bytes: &Arc<Mutex<std::vec::Vec<u8>>>, capacity: usize, frame: &[u8]) -> bool {
    let write = usize::from(read16(bytes, 6));
    let read = usize::from(read16(bytes, 4));
    let free = capacity - (write + capacity - read) % capacity;
    if (free == capacity && read8(bytes, 3) != 1) || frame.len() > free {
        return false;
    }
    for (i, b) in frame.iter().enumerate() {
        write8(bytes, 12 + (write + i) % capacity, *b);
        if i % 7 == 0 {
            std::thread::yield_now();
        }
    }
    write16(bytes, 6, ((write + frame.len()) % capacity) as u16);
    if read8(bytes, 3) == 1 {
        std::thread::yield_now();
        write8(bytes, 3, 0);
    }
    true
}

#[test]
fn concurrent_application_producer_matches_original_sdk_consumer_without_flag_writes() {
    for capacity in [65, 244, 1268, 65536] {
        let memory = prepared(capacity, true);
        let bytes = memory.bytes.clone();
        let drops = memory.drops.clone();
        let mut tx = unsafe { Transmit::from_prepared(memory, Channel::Channel1) }.unwrap();
        let peer = std::thread::spawn(move || {
            let end = Instant::now() + Duration::from_secs(15);
            for n in 0..1000 {
                loop {
                    if let Some(output) = sdk_pop(&bytes, capacity) {
                        assert_eq!(output, wire(n, n % 61));
                        break;
                    }
                    assert!(Instant::now() < end, "SDK consumer timed out");
                    std::thread::yield_now();
                }
            }
        });
        let end = Instant::now() + Duration::from_secs(15);
        for n in 0..1000 {
            let input = wire(n, n % 61);
            loop {
                match unsafe { tx.try_push(decode(&input)) } {
                    Ok(()) => break,
                    Err(Error::Mailbox(MailboxError::Full)) => {
                        assert!(Instant::now() < end, "Rust producer timed out");
                        std::thread::yield_now();
                    }
                    Err(e) => panic!("unexpected: {e:?}"),
                }
            }
        }
        peer.join().unwrap();
        assert_eq!(tx.memory.snapshot().empty, 1);
        assert_eq!(tx.memory.snapshot().read, tx.memory.snapshot().write);
        drop(unsafe { tx.reclaim_after_peer_quiescent() });
        assert_eq!(drops.load(AtomicOrdering::SeqCst), 1);
    }
}

#[test]
fn concurrent_original_sdk_producer_matches_biased_application_consumer() {
    for capacity in [65, 244, 1268, 65536] {
        let memory = prepared(capacity, false);
        let bytes = memory.bytes.clone();
        let drops = memory.drops.clone();
        let mut rx = unsafe { Receive::from_prepared(memory, Channel::Channel1) }.unwrap();
        let peer = std::thread::spawn(move || {
            let end = Instant::now() + Duration::from_secs(15);
            for n in 0..1000 {
                let input = wire(n, n % 61);
                while !sdk_push(&bytes, capacity, &input) {
                    assert!(Instant::now() < end, "SDK producer timed out");
                    std::thread::yield_now();
                }
            }
        });
        let mut output = [0xa5; MAX_MESSAGE];
        let end = Instant::now() + Duration::from_secs(15);
        for n in 0..1000 {
            loop {
                if let Some(actual) = rx.try_pop_into(&mut output).unwrap() {
                    assert_eq!(actual, decode(&wire(n, n % 61)));
                    break;
                }
                assert!(Instant::now() < end, "Rust consumer timed out");
                std::thread::yield_now();
            }
        }
        peer.join().unwrap();
        assert_eq!(rx.try_pop_into(&mut output), Ok(None));
        let s = rx.memory.snapshot();
        assert_eq!(s.empty, 0);
        assert_eq!((usize::from(s.read) + 1) % capacity, usize::from(s.write));
        drop(unsafe { rx.reclaim_after_peer_quiescent() });
        assert_eq!(drops.load(AtomicOrdering::SeqCst), 1);
    }
}

#[test]
#[allow(clippy::drop_non_drop)] // Exercise the intentional no-cleanup drop contract.
fn drop_retains_allocation_and_explicit_quiescent_reclaim_is_required() {
    let memory = prepared(244, true);
    let drops = memory.drops.clone();
    let tx = unsafe { Transmit::from_prepared(memory, Channel::Channel1) }.unwrap();
    drop(tx);
    assert_eq!(drops.load(AtomicOrdering::SeqCst), 0);
    let memory = prepared(244, false);
    let drops = memory.drops.clone();
    let rx = unsafe { Receive::from_prepared(memory, Channel::Channel1) }.unwrap();
    drop(rx);
    assert_eq!(drops.load(AtomicOrdering::SeqCst), 0);
}

#[test]
fn bad_handoff_metadata_and_local_cursor_changes_are_rejected() {
    for tx in [false, true] {
        for (offset, value) in [
            (0, 255),
            (1, 255),
            (2, 255),
            (3, if tx { 0 } else { 1 }),
            (4, 244),
            (6, 244),
            (8, 0),
            (9, 1),
        ] {
            let memory = prepared(244, tx);
            let before = memory.bytes.clone();
            write8(&before, offset, value);
            let snapshot = before.lock().unwrap().clone();
            let error = if tx {
                unsafe { Transmit::from_prepared(memory, Channel::Channel1) }.err()
            } else {
                unsafe { Receive::from_prepared(memory, Channel::Channel1) }.err()
            };
            assert!(error.is_some());
            assert_eq!(*before.lock().unwrap(), snapshot);
        }
    }
    let memory = prepared(244, true);
    let bytes = memory.bytes.clone();
    let mut tx = unsafe { Transmit::from_prepared(memory, Channel::Channel1) }.unwrap();
    write16(&bytes, 6, 1);
    assert_eq!(
        unsafe { tx.try_push(decode(&wire(0, 0))) },
        Err(Error::LocalCursorChanged)
    );
    drop(unsafe { tx.reclaim_after_peer_quiescent() });
    let memory = prepared(244, false);
    let bytes = memory.bytes.clone();
    let mut rx = unsafe { Receive::from_prepared(memory, Channel::Channel1) }.unwrap();
    write16(&bytes, 4, 0);
    assert_eq!(
        rx.try_pop_into(&mut [0; MAX_MESSAGE]),
        Err(Error::LocalCursorChanged)
    );
    drop(unsafe { rx.reclaim_after_peer_quiescent() });
}

#[test]
fn full_and_corrupt_publications_do_not_advance_or_change_output() {
    let memory = prepared(244, true);
    let bytes = memory.bytes.clone();
    let mut tx = unsafe { Transmit::from_prepared(memory, Channel::Channel1) }.unwrap();
    for length in [60, 60, 60, 47] {
        unsafe { tx.try_push(decode(&wire(18, length))) }.unwrap();
    }
    let before = bytes.lock().unwrap().clone();
    assert_eq!(
        unsafe { tx.try_push(decode(&wire(0, 0))) },
        Err(Error::Mailbox(MailboxError::Full))
    );
    assert_eq!(*bytes.lock().unwrap(), before);
    assert_eq!(read8(&bytes, 3), 1);
    assert_ne!(read16(&bytes, 4), read16(&bytes, 6));
    drop(unsafe { tx.reclaim_after_peer_quiescent() });
    for (frame, published, error) in [
        ([18, 1, 0, 0], 3, MailboxError::Incomplete),
        ([18, 1, 60, 0], 4, MailboxError::Incomplete),
        (
            [18, 1, 61, 0],
            4,
            MailboxError::Message(MessageError::PayloadTooLong),
        ),
        (
            [255, 1, 0, 0],
            4,
            MailboxError::Message(MessageError::UnknownId),
        ),
        (
            [18, 255, 0, 0],
            4,
            MailboxError::Message(MessageError::UnknownPriority),
        ),
    ] {
        let memory = prepared(244, false);
        let bytes = memory.bytes.clone();
        let mut rx = unsafe { Receive::from_prepared(memory, Channel::Channel1) }.unwrap();
        bytes.lock().unwrap()[12..16].copy_from_slice(&frame);
        write16(&bytes, 6, published);
        let before = bytes.lock().unwrap().clone();
        let mut output = [0xa5; MAX_MESSAGE];
        assert_eq!(rx.try_pop_into(&mut output), Err(Error::Mailbox(error)));
        assert_eq!(*bytes.lock().unwrap(), before);
        assert_eq!(output, [0xa5; MAX_MESSAGE]);
        drop(unsafe { rx.reclaim_after_peer_quiescent() });
    }
}

#[test]
fn volatile_adapter_uses_real_aligned_host_ram_and_keeps_guard_byte() {
    let storage = std::boxed::Box::into_raw(std::vec![0xa5a5_a5a5u32; 20].into_boxed_slice());
    let base = NonNull::new(storage.cast::<u32>().cast::<u8>()).unwrap();
    let size = MailboxSize::try_from_total_bytes(76).unwrap();
    let initial = prepared(64, true).bytes.lock().unwrap().clone();
    // Ordinary RAM only, not a hardware mapping or real peer qualification.
    unsafe { core::ptr::copy_nonoverlapping(initial.as_ptr(), base.as_ptr(), initial.len()) };
    let misaligned = unsafe { NonNull::new_unchecked(base.as_ptr().add(1)) };
    assert!(matches!(
        unsafe { CoherentMemory::from_mapping(misaligned, size) },
        Err(Error::Mapping)
    ));
    let memory = unsafe { CoherentMemory::from_mapping(base, size) }.unwrap();
    let mut tx = unsafe { Transmit::from_prepared(memory, Channel::Channel1) }.unwrap();
    assert_eq!(
        unsafe { tx.try_push(decode(&wire(18, 60))) },
        Err(Error::Mailbox(MailboxError::Full))
    );
    unsafe { tx.try_push(decode(&wire(18, 59))) }.unwrap();
    let memory = unsafe { tx.reclaim_after_peer_quiescent() };
    assert_eq!(memory.snapshot().write, 63);
    assert_eq!(memory.snapshot().empty, 1);
    #[allow(clippy::drop_non_drop)] // End the tested exclusive view before reclaiming its storage.
    drop(memory);
    let contents = unsafe { core::slice::from_raw_parts(base.as_ptr(), 76) };
    assert_eq!(&contents[12..75], wire(18, 59));
    assert_eq!(contents[75], 0xa5);
    drop(unsafe { std::boxed::Box::from_raw(storage) });
}
