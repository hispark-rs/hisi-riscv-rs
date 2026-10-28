//! Concurrent storage for the specifically qualified SDK peer algorithms.
//!
//! TX leaves one byte free and never clears empty1. The SDK consumer examines
//! cursors first and can drain a differing pair even with empty1. RX biases its
//! stored read cursor one byte behind the next header, leaves empty0 and never
//! writes that flag. The SDK producer then accounts for one occupied sentinel
//! byte and cannot overwrite unread data. Neither role needs the peer to change
//! its code, **provided its actual algorithm matches this contract**.
//!
//! This is not a generic claim about installed Ccore firmware. Preparation and
//! ownership handoff require a quiescent peer; an old SDK ISR must not share the
//! local cursor. Both mappings must be coherent/uncached, with no cached alias.
//! There is no clock/reset, service negotiation or notification/rearm here.

use super::mailbox::{MailboxError, MailboxSize};
use super::{Channel, MAX_MESSAGE, MAX_PAYLOAD, Message, MessageError};
use core::sync::atomic::{Ordering, compiler_fence};
use core::{marker::PhantomData, mem::ManuallyDrop, ptr::NonNull};
use vcell::VolatileCell;

/// Observed metadata. Fields are untrusted input, never address selectors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Snapshot {
    /// Wire core IDs and channel, in that order.
    pub identity: [u8; 3],
    /// Wire empty byte, validated against the role's constant.
    pub empty: u8,
    /// Peer/local stored read cursor, not necessarily the RX logical head.
    pub read: u16,
    /// Stored write cursor.
    pub write: u16,
    /// Stored capacity in bytes, excluding twelve metadata bytes.
    pub capacity: u32,
}

/// Shared ordinary-memory access, not an MMIO register bank.
///
/// # Safety
/// Keep the complete extent valid, initialized and identity/capacity immutable.
/// Halfword cursor loads/stores must be single-copy atomic and aligned. Loads
/// acquire peer publication; cursor stores release all preceding data accesses.
/// Byte accesses must be exact and observable to the peer. Both mappings must
/// be coherent or uncached without incoherent aliases; volatile alone does not
/// flush caches. Do not perform whole-line cache writeback over peer-owned data.
/// This object exclusively owns this role's local accesses; the sole external
/// peer uses only its complementary fields. No DMA or extra core may race it.
/// The peer must also acquire the opposite cursor and release its own cursor
/// after the required data accesses, via barriers or a proven stronger hardware
/// ordering rule. Uncached/coherent visibility alone does not prove this order.
/// A successful attachment retains this object on Drop/forget; its allocation
/// and any external mappings must remain valid until explicit quiescent reclaim.
pub unsafe trait SharedMailbox: 'static {
    /// Stable declared allocation extent; not a value supplied by the peer.
    fn size(&self) -> MailboxSize;
    /// Acquire a peer cursor before later data/free-space accesses. Fields need
    /// not be one atomic multiword snapshot; only one cursor is peer-written.
    fn snapshot(&self) -> Snapshot;
    /// Read one byte of published ring data.
    /// # Safety
    /// index < capacity and the producer must not reuse this unread byte.
    unsafe fn load_data(&self, index: usize) -> u8;
    /// Write one byte in reserved free ring data.
    /// # Safety
    /// index < capacity; own this producer role and reserve the byte first.
    unsafe fn store_data(&mut self, index: usize, value: u8);
    /// Release a completed message by updating only write_index.
    /// # Safety
    /// Own this writer, cursor < capacity, complete data already stored, and
    /// every newly visible service payload/remote reference authorized.
    unsafe fn publish_write(&mut self, cursor: u16);
    /// Release copied data by updating only read_index.
    /// # Safety
    /// Own this reader, cursor < capacity, all consumed bytes already copied.
    unsafe fn release_read(&mut self, cursor: u16);
}

/// Invalid mapping or protocol. No cursor is advanced on an error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// Metadata/frame validation failure, including insufficient capacity.
    Mailbox(MailboxError),
    /// Local cursor changed outside this owner (for example peer reset/old ISR).
    LocalCursorChanged,
    /// Mapping base is not four-byte aligned or its address extent wraps.
    Mapping,
}

/// Volatile ordinary RAM adapter, intentionally !Send/!Sync.
/// Does not own or allocate the physical region and never initializes metadata.
pub struct CoherentMemory {
    base: NonNull<u8>,
    size: MailboxSize,
}
impl CoherentMemory {
    /// Adopt a platform-owned mapping; validate address arithmetic only.
    ///
    /// # Safety
    /// Supply initialized ordinary shared RAM of exactly `size`, accessible to
    /// both roles, valid through explicit peer quiescence even after Drop/forget.
    /// No Rust references/other owners may alias live data, except ephemeral
    /// UnsafeCell-backed accesses under this protocol. Both core mappings must
    /// be coherent/uncached with no cached alias. Ensure aligned atomic u16
    /// cursor accesses, accessible power/clock and no remap/reset while attached.
    /// This does not validate PMP attributes, allocate RAM or claim a peer lease.
    pub unsafe fn from_mapping(base: NonNull<u8>, size: MailboxSize) -> Result<Self, Error> {
        if !base.as_ptr().addr().is_multiple_of(4)
            || base
                .as_ptr()
                .addr()
                .checked_add(size.total_bytes())
                .is_none()
        {
            return Err(Error::Mapping);
        }
        Ok(Self { base, size })
    }
    fn byte(&self, offset: usize) -> &VolatileCell<u8> {
        debug_assert!(offset < self.size.total_bytes());
        // SAFETY: mapping contract, bounded offset; UnsafeCell permits the
        // external peer's complementary accesses without ordinary references.
        unsafe { &*self.base.as_ptr().add(offset).cast() }
    }
    fn half(&self, offset: usize) -> &VolatileCell<u16> {
        debug_assert!(offset == 4 || offset == 6);
        // SAFETY: four-aligned base and the two fixed aligned cursor cells.
        unsafe { &*self.base.as_ptr().add(offset).cast() }
    }
    fn word(&self) -> &VolatileCell<u32> {
        // SAFETY: four-aligned mapping contains the capacity word at offset8.
        unsafe { &*self.base.as_ptr().add(8).cast() }
    }
}
fn acquire() {
    #[cfg(target_arch = "riscv32")]
    // SAFETY: memory ordering only; no MMIO, pointer or atomic extension.
    unsafe {
        core::arch::asm!("fence rw, rw", options(nostack))
    };
    compiler_fence(Ordering::Acquire);
}
fn release() {
    compiler_fence(Ordering::Release);
    #[cfg(target_arch = "riscv32")]
    // SAFETY: order ordinary shared-memory data before cursor publication.
    unsafe {
        core::arch::asm!("fence rw, rw", options(nostack))
    };
}
// SAFETY: constructor adopts the externally qualified mapping; volatile cells
// preserve exact access widths and barriers implement publication order. No
// Send/Sync or cache-coherence guarantee is invented by those barriers.
unsafe impl SharedMailbox for CoherentMemory {
    fn size(&self) -> MailboxSize {
        self.size
    }
    fn snapshot(&self) -> Snapshot {
        let s = Snapshot {
            identity: [self.byte(0).get(), self.byte(1).get(), self.byte(2).get()],
            empty: self.byte(3).get(),
            read: self.half(4).get(),
            write: self.half(6).get(),
            capacity: self.word().get(),
        };
        acquire();
        s
    }
    unsafe fn load_data(&self, index: usize) -> u8 {
        self.byte(12 + index).get()
    }
    unsafe fn store_data(&mut self, index: usize, value: u8) {
        self.byte(12 + index).set(value);
    }
    unsafe fn publish_write(&mut self, cursor: u16) {
        release();
        self.half(6).set(cursor);
    }
    unsafe fn release_read(&mut self, cursor: u16) {
        release();
        self.half(4).set(cursor);
    }
}

fn identity(channel: Channel, tx: bool) -> [u8; 3] {
    [
        if tx { 2 } else { 0 },
        if tx { 0 } else { 2 },
        match channel {
            Channel::Channel0 => 0,
            Channel::Channel1 => 1,
        },
    ]
}
fn validate(s: Snapshot, size: MailboxSize, channel: Channel, tx: bool) -> Result<(), Error> {
    let error = if s.identity != identity(channel, tx) {
        Some(MailboxError::Route)
    } else if size.capacity() < 5 || s.capacity != size.capacity() as u32 {
        Some(MailboxError::Capacity)
    } else if usize::from(s.read) >= size.capacity() || usize::from(s.write) >= size.capacity() {
        Some(MailboxError::Cursor)
    } else if s.empty != u8::from(tx) {
        Some(MailboxError::EmptyFlag)
    } else {
        None
    };
    error.map_or(Ok(()), |e| Err(Error::Mailbox(e)))
}

/// Application producer, SDK consumer. One byte remains free, empty stays1.
/// Drop retains the mapping/backend; only unsafe quiescent reclaim releases it.
pub struct Transmit<M: SharedMailbox> {
    memory: ManuallyDrop<M>,
    size: MailboxSize,
    channel: Channel,
    write: u16,
    _local: PhantomData<*mut ()>,
}
impl<M: SharedMailbox> Transmit<M> {
    /// Attach to a prepared empty TX mailbox without writing metadata.
    ///
    /// # Safety
    /// Quiesce the actual peer and old local handlers for handoff. Ensure exactly
    /// one local producer and a peer using the evidenced cursor-first SDK
    /// consumer (not an empty-byte fast path), never resetting/initializing while
    /// attached. Metadata has empty1 and equal cursors; mappings obey
    /// SharedMailbox. Start the peer only after success. This function does not
    /// prove the installed Ccore algorithm, service permissions or notification.
    /// Establish the peer's acquire-before-copy/reuse and release-after-copy
    /// ordering as well; local Rust fences cannot repair an unordered peer.
    pub unsafe fn from_prepared(memory: M, channel: Channel) -> Result<Self, Error> {
        let size = memory.size();
        let s = memory.snapshot();
        validate(s, size, channel, true)?;
        if s.read != s.write {
            return Err(Error::LocalCursorChanged);
        }
        Ok(Self {
            memory: ManuallyDrop::new(memory),
            size,
            channel,
            write: s.write,
            _local: PhantomData,
        })
    }
    /// Publish one message, retaining one free byte. Returns Full without changes.
    /// Notification/retry is separate; peer may see data before any doorbell.
    /// # Safety
    /// Authorize the ID/service and every embedded remote pointer, size and
    /// buffer lifetime for the actual peer. A validated frame alone grants none.
    pub unsafe fn try_push(&mut self, message: Message<'_>) -> Result<(), Error> {
        let s = self.memory.snapshot();
        validate(s, self.size, self.channel, true)?;
        if s.write != self.write {
            return Err(Error::LocalCursorChanged);
        }
        let capacity = self.size.capacity();
        let write = usize::from(self.write);
        let used = (write + capacity - usize::from(s.read)) % capacity;
        let mut frame = [0; MAX_MESSAGE];
        let length = message.encode(&mut frame);
        if length > capacity - 1 - used {
            return Err(Error::Mailbox(MailboxError::Full));
        }
        for (i, byte) in frame[..length].iter().copied().enumerate() {
            // SAFETY: validated extent, reserved free bytes; consumer can only
            // release more space. Sole producer, caller authorized publication.
            unsafe { self.memory.store_data((write + i) % capacity, byte) };
        }
        let next = ((write + length) % capacity) as u16;
        // SAFETY: all message bytes stored, valid cursor; release orders data.
        unsafe { self.memory.publish_write(next) };
        self.write = next;
        // Crucially no empty0 store, even when transitioning from empty.
        Ok(())
    }
    /// Recover the retained backend only after peer and all other users stop.
    /// # Safety
    /// Prove no peer access, pending notification or borrowed payload references
    /// remain, and reconcile mapping/cache ownership before reusing allocation.
    pub unsafe fn reclaim_after_peer_quiescent(self) -> M {
        ManuallyDrop::into_inner(self.memory)
    }
}

/// Application consumer, SDK producer. Stored read is one behind logical head.
/// Empty stays0; the producer accounts for one permanently occupied guard byte.
pub struct Receive<M: SharedMailbox> {
    memory: ManuallyDrop<M>,
    size: MailboxSize,
    channel: Channel,
    read: u16,
    _local: PhantomData<*mut ()>,
}
impl<M: SharedMailbox> Receive<M> {
    /// Attach to a prepared empty RX mailbox without changing any byte.
    ///
    /// # Safety
    /// Quiesce both roles for handoff, after the peer's own initialization. The
    /// sole peer producer must match the evidenced SDK free-space/empty test
    /// and conditional empty0 store; it must not reset/reinitialize, inspect
    /// read_index as application data, or require a different empty fast path.
    /// Prepare empty0, write=logical head, read=(head+capacity-1)%capacity, exact
    /// source/control0 destination/app2 and channel. Exclude old local SDK users.
    /// Prove peer release-before-W and acquire-before-reuse ordering through
    /// its barriers or the actual hardware mapping; coherency alone is not it.
    /// SharedMailbox and retained lifetime contracts apply. Start peer only
    /// after success; installed Ccore compatibility remains platform evidence.
    pub unsafe fn from_prepared(memory: M, channel: Channel) -> Result<Self, Error> {
        let size = memory.size();
        let s = memory.snapshot();
        validate(s, size, channel, false)?;
        if (usize::from(s.read) + 1) % size.capacity() != usize::from(s.write) {
            return Err(Error::LocalCursorChanged);
        }
        Ok(Self {
            memory: ManuallyDrop::new(memory),
            size,
            channel,
            read: s.read,
            _local: PhantomData,
        })
    }
    /// Bounded acquire/copy/validate/release of one complete inline message.
    /// Errors retain cursors and output. No callback/remote pointer is invoked.
    pub fn try_pop_into<'out>(
        &mut self,
        output: &'out mut [u8; MAX_MESSAGE],
    ) -> Result<Option<Message<'out>>, Error> {
        let s = self.memory.snapshot();
        validate(s, self.size, self.channel, false)?;
        if s.read != self.read {
            return Err(Error::LocalCursorChanged);
        }
        let capacity = self.size.capacity();
        let head = (usize::from(self.read) + 1) % capacity;
        let available = (usize::from(s.write) + capacity - head) % capacity;
        if available == 0 {
            return Ok(None);
        }
        if available < 4 {
            return Err(Error::Mailbox(MailboxError::Incomplete));
        }
        let mut frame = [0; MAX_MESSAGE];
        for (i, byte) in frame[..4].iter_mut().enumerate() {
            // SAFETY: acquired published region; producer cannot reuse it
            // until this consumer releases the read cursor below.
            *byte = unsafe { self.memory.load_data((head + i) % capacity) };
        }
        let payload = usize::from(u16::from_le_bytes([frame[2], frame[3]]));
        if payload > MAX_PAYLOAD {
            return Err(Error::Mailbox(MailboxError::Message(
                MessageError::PayloadTooLong,
            )));
        }
        let length = payload + 4;
        if length > available {
            return Err(Error::Mailbox(MailboxError::Incomplete));
        }
        for (i, byte) in frame[4..length].iter_mut().enumerate() {
            // SAFETY: same acquired complete message and checked bounded index.
            *byte = unsafe { self.memory.load_data((head + 4 + i) % capacity) };
        }
        Message::decode(&frame[..length]).map_err(|e| Error::Mailbox(MailboxError::Message(e)))?;
        output[..length].copy_from_slice(&frame[..length]);
        let next = ((usize::from(self.read) + length) % capacity) as u16;
        // SAFETY: copy completed, valid biased read cursor; release prevents
        // peer reuse until every source read has finished. Empty remains0.
        unsafe { self.memory.release_read(next) };
        self.read = next;
        Ok(Some(Message::decode(&output[..length]).unwrap()))
    }
    /// Recover retained mapping/backend after the peer stops accessing it.
    /// # Safety
    /// Same full quiescence/cache/borrow proof as Transmit's reclaim method.
    pub unsafe fn reclaim_after_peer_quiescent(self) -> M {
        ManuallyDrop::into_inner(self.memory)
    }
}

#[cfg(test)]
#[path = "spsc_tests.rs"]
mod tests;
