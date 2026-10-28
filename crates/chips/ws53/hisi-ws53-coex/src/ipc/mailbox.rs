//! Bounded SDK-layout ring transactions, with exclusive access to an image.
//!
//! The twelve-byte header and cursor rules follow the pinned WS53 ROM. All
//! references here are to exclusively owned ordinary memory. In particular,
//! **do not** construct an `&mut [u8]` into live shared RAM while the control core
//! or another bus master may access it. A local interrupt mask is insufficient.
//! This layer supplies neither cross-core exclusion nor cache maintenance,
//! notification/rearm, live-buffer handoff or remote service authorization.
//!
//! Both ROM roles write the empty byte. Its full/empty ambiguity must be resolved
//! by a qualified transport before these serialized operations can run against
//! a live peer. No lock-free/shared-memory claim is made by this safe image API.

use super::{Channel, MAX_MESSAGE, Message, MessageError};

const METADATA: usize = 12;

/// Valid image extent. The maximum capacity is one complete u16 cursor cycle.
/// These are representability bounds, not a claim about board RAM allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct MailboxSize(usize);
impl MailboxSize {
    /// Include twelve metadata bytes and at least one four-byte message header.
    pub const fn try_from_total_bytes(bytes: usize) -> Option<Self> {
        if bytes >= METADATA + 4 && bytes <= METADATA + 65536 {
            Some(Self(bytes))
        } else {
            None
        }
    }
    /// Exact extent required by attachment/initialization.
    pub const fn total_bytes(self) -> usize {
        self.0
    }
    /// Number of modulo-addressed ring bytes, excluding metadata.
    pub const fn capacity(self) -> usize {
        self.0 - METADATA
    }
}

/// Explicit source/destination identity; not a raw core number or bus address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Route {
    /// SDK core2 sends to core0.
    ApplicationToControl(Channel),
    /// SDK core0 sends to core2.
    ControlToApplication(Channel),
}
impl Route {
    fn header(self) -> [u8; 3] {
        let (src, dst, channel) = match self {
            Self::ApplicationToControl(c) => (2, 0, c),
            Self::ControlToApplication(c) => (0, 2, c),
        };
        [
            src,
            dst,
            match channel {
                Channel::Channel0 => 0,
                Channel::Channel1 => 1,
            },
        ]
    }
}

/// Rejected before any image cursor, message or caller output is changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum MailboxError {
    /// The backing slice does not exactly match the declared extent.
    StorageSize,
    /// Source/destination/channel differs from the typed expected route.
    Route,
    /// Stored capacity does not match the supplied extent.
    Capacity,
    /// A cursor lies outside the ring.
    Cursor,
    /// Empty flag is not an SDK boolean0/1.
    EmptyFlag,
    /// Insufficient free bytes; nothing was enqueued.
    Full,
    /// Published bytes do not contain one complete declared message.
    Incomplete,
    /// Invalid bounded message format; no dispatch or consumption occurred.
    Message(MessageError),
}

struct State {
    read: usize,
    write: usize,
    used: usize,
}

/// A serialized, exclusively owned SDK mailbox image. Not a shared RAM token.
/// Drop changes nothing. There is no automatic recovery/clearing of corruption.
pub struct MailboxImage<'a> {
    bytes: &'a mut [u8],
    size: MailboxSize,
    route: Route,
}
impl<'a> MailboxImage<'a> {
    /// Attach without changing any byte. Validate metadata, not queued messages.
    pub fn attach(
        bytes: &'a mut [u8],
        size: MailboxSize,
        route: Route,
    ) -> Result<Self, MailboxError> {
        if bytes.len() != size.total_bytes() {
            return Err(MailboxError::StorageSize);
        }
        let image = Self { bytes, size, route };
        image.state()?;
        Ok(image)
    }

    /// Initialize only metadata of an exclusively owned image; old ring bytes
    /// are left untouched. Never use this to reset a live peer's mailbox.
    pub fn initialize(
        bytes: &'a mut [u8],
        size: MailboxSize,
        route: Route,
    ) -> Result<Self, MailboxError> {
        if bytes.len() != size.total_bytes() {
            return Err(MailboxError::StorageSize);
        }
        bytes[..3].copy_from_slice(&route.header());
        bytes[3] = 1;
        bytes[4..8].fill(0);
        // Validated capacity <=65536; this conversion cannot truncate.
        bytes[8..12].copy_from_slice(&(size.capacity() as u32).to_le_bytes());
        Ok(Self { bytes, size, route })
    }

    fn state(&self) -> Result<State, MailboxError> {
        if self.bytes[..3] != self.route.header() {
            return Err(MailboxError::Route);
        }
        let capacity = self.size.capacity();
        let stored_capacity = u32::from_le_bytes(self.bytes[8..12].try_into().unwrap());
        if stored_capacity != capacity as u32 {
            return Err(MailboxError::Capacity);
        }
        let read = usize::from(u16::from_le_bytes([self.bytes[4], self.bytes[5]]));
        let write = usize::from(u16::from_le_bytes([self.bytes[6], self.bytes[7]]));
        if read >= capacity || write >= capacity {
            return Err(MailboxError::Cursor);
        }
        if self.bytes[3] > 1 {
            return Err(MailboxError::EmptyFlag);
        }
        // Match ROM: when cursors differ, the empty byte is ignored. An equal
        // pair uses empty0 for full and empty1 for empty. This is only sound
        // under the exclusive-transaction contract, not as a racing snapshot.
        let used = if read == write && self.bytes[3] == 0 {
            capacity
        } else {
            (write + capacity - read) % capacity
        };
        Ok(State { read, write, used })
    }

    /// Published ring byte count, including headers. Does not validate frames.
    pub fn queued_bytes(&self) -> Result<usize, MailboxError> {
        Ok(self.state()?.used)
    }

    /// Free ring byte count. A successful push may exactly fill the image.
    pub fn free_bytes(&self) -> Result<usize, MailboxError> {
        Ok(self.size.capacity() - self.state()?.used)
    }

    /// Serialize one validated inline message, then commit the write cursor and
    /// clear empty. No doorbell, buffer ownership transfer or cache operation.
    pub fn try_push(&mut self, message: Message<'_>) -> Result<(), MailboxError> {
        let state = self.state()?;
        let mut frame = [0; MAX_MESSAGE];
        let length = message.encode(&mut frame);
        let capacity = self.size.capacity();
        if length > capacity - state.used {
            return Err(MailboxError::Full);
        }
        for (i, byte) in frame[..length].iter().copied().enumerate() {
            self.bytes[METADATA + (state.write + i) % capacity] = byte;
        }
        let next = (state.write + length) % capacity;
        // next < capacity <=65536, so every committed cursor fits in u16.
        self.bytes[6..8].copy_from_slice(&(next as u16).to_le_bytes());
        self.bytes[3] = 0;
        Ok(())
    }

    /// Copy and validate exactly one message before changing the read cursor or
    /// caller output. An empty queue returns None. Corruption is retained for
    /// explicit recovery; no skip, automatic clear, pointer dereference or call
    /// through the peer-supplied discriminator is performed.
    pub fn try_pop_into<'out>(
        &mut self,
        output: &'out mut [u8; MAX_MESSAGE],
    ) -> Result<Option<Message<'out>>, MailboxError> {
        let state = self.state()?;
        if state.used == 0 {
            return Ok(None);
        }
        if state.used < 4 {
            return Err(MailboxError::Incomplete);
        }
        let capacity = self.size.capacity();
        let mut frame = [0; MAX_MESSAGE];
        for (i, byte) in frame[..4].iter_mut().enumerate() {
            *byte = self.bytes[METADATA + (state.read + i) % capacity];
        }
        let payload_length = usize::from(u16::from_le_bytes([frame[2], frame[3]]));
        if payload_length > super::MAX_PAYLOAD {
            return Err(MailboxError::Message(MessageError::PayloadTooLong));
        }
        let length = 4 + payload_length;
        if length > state.used {
            return Err(MailboxError::Incomplete);
        }
        for (i, byte) in frame[4..length].iter_mut().enumerate() {
            *byte = self.bytes[METADATA + (state.read + 4 + i) % capacity];
        }
        Message::decode(&frame[..length]).map_err(MailboxError::Message)?;
        output[..length].copy_from_slice(&frame[..length]);
        let next = (state.read + length) % capacity;
        self.bytes[4..6].copy_from_slice(&(next as u16).to_le_bytes());
        if next == state.write {
            self.bytes[3] = 1;
        }
        // The same frame was validated above, and exact bytes were copied.
        Ok(Some(Message::decode(&output[..length]).unwrap()))
    }

    /// Borrow the exclusive image for inspection; no mutable alias is granted.
    pub fn bytes(&self) -> &[u8] {
        self.bytes
    }
}

#[cfg(test)]
#[path = "mailbox_tests.rs"]
mod tests;
