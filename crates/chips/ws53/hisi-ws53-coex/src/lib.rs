#![no_std]
//! WS53 vendor firmware cooperation protocols; no peer ownership is implied.

#[cfg(test)]
extern crate std;

/// Channel number encoded in the vendor cooperation protocol. The platform
/// adapter must explicitly map it to the physical HAL doorbell channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Channel {
    Channel0,
    Channel1,
}

/// Maximum inline payload accepted by the SDK application IPC profile.
pub const MAX_PAYLOAD: usize = 60;
/// Header plus maximum inline payload; excludes the twelve-byte ring metadata.
pub const MAX_MESSAGE: usize = 4 + MAX_PAYLOAD;

/// Valid SDK message discriminator. This grants no permission to invoke a service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct MessageId(u8);
impl MessageId {
    /// Accept only the nineteen IDs in the fixed SDK protocol.
    pub const fn try_from_raw(raw: u8) -> Option<Self> {
        if raw < 19 { Some(Self(raw)) } else { None }
    }
    /// SDK IPC_MSG_UPDATE_SHARE_RANDOM_NUMBER discriminator.
    pub const UPDATE_SHARED_RANDOM_NUMBER: Self = Self(18);
    /// Wire discriminator; never interpreted as a callback or pointer.
    pub const fn raw(self) -> u8 {
        self.0
    }
}

/// SDK relative message priority. Channel choice is independent of priority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum Priority {
    /// Highest priority, wire value0.
    High = 0,
    /// Lowest priority, wire value1.
    Low = 1,
}

/// Defensive message format rejection; no shared memory is consumed on error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum MessageError {
    /// A payload or its declared length exceeds the SDK sixty-byte bound.
    PayloadTooLong,
    /// Missing header, truncated payload or trailing data.
    Framing,
    /// Discriminator is outside the SDK callback-table range.
    UnknownId,
    /// Priority is neither zero nor one.
    UnknownPriority,
}

/// Validated inline message borrowing ordinary application memory.
///
/// This is not a reference into concurrently written shared memory. First copy
/// a complete, coherently published message into locally owned storage under a
/// qualified ring protocol, then decode. No embedded address is dereferenced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Message<'a> {
    id: MessageId,
    priority: Priority,
    payload: &'a [u8],
}
impl<'a> Message<'a> {
    /// Construct an inline message; reject overlength without narrowing/clamping.
    pub fn new(id: MessageId, priority: Priority, payload: &'a [u8]) -> Result<Self, MessageError> {
        if payload.len() > MAX_PAYLOAD {
            return Err(MessageError::PayloadTooLong);
        }
        Ok(Self {
            id,
            priority,
            payload,
        })
    }
    /// Validate exactly one header and payload from locally owned memory.
    pub fn decode(bytes: &'a [u8]) -> Result<Self, MessageError> {
        if bytes.len() < 4 {
            return Err(MessageError::Framing);
        }
        let id = MessageId::try_from_raw(bytes[0]).ok_or(MessageError::UnknownId)?;
        let priority = match bytes[1] {
            0 => Priority::High,
            1 => Priority::Low,
            _ => return Err(MessageError::UnknownPriority),
        };
        let length = u16::from_le_bytes([bytes[2], bytes[3]]) as usize;
        if length > MAX_PAYLOAD {
            return Err(MessageError::PayloadTooLong);
        }
        if bytes.len() != 4 + length {
            return Err(MessageError::Framing);
        }
        Self::new(id, priority, &bytes[4..])
    }
    /// Encode one four-byte little-endian header and payload; return used length.
    /// Bytes beyond the returned prefix remain untouched.
    pub fn encode(self, destination: &mut [u8; MAX_MESSAGE]) -> usize {
        // Construction/decoding bounds length to60 before this conversion.
        let length = self.payload.len() as u16;
        destination[..4].copy_from_slice(&[
            self.id.raw(),
            self.priority as u8,
            length as u8,
            (length >> 8) as u8,
        ]);
        destination[4..4 + self.payload.len()].copy_from_slice(self.payload);
        4 + self.payload.len()
    }
    /// Validated wire discriminator; does not grant service ownership.
    pub const fn id(self) -> MessageId {
        self.id
    }
    /// Validated priority.
    pub const fn priority(self) -> Priority {
        self.priority
    }
    /// Locally owned input slice, never an unchecked peer pointer.
    pub const fn payload(self) -> &'a [u8] {
        self.payload
    }
}

#[path = "ipc/layout.rs"]
pub mod layout;
#[path = "ipc/mailbox.rs"]
pub mod mailbox;
#[path = "ipc/random_buffer.rs"]
pub mod random_buffer;
#[path = "ipc/spsc.rs"]
pub mod spsc;

#[cfg(test)]
mod message_tests;
