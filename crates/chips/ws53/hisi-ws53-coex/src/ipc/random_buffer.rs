//! SDK IPC18: control core requests application-side TRNG buffer updates.
//!
//! Source: SDK163d9150 `trng_ipc_acore_buffer.c`. The twelve-byte payload is
//! three little-endian words: update kind, control-local start, whole length.
//! There is no documented reply message here. Completing an image update does
//! not acknowledge a doorbell, publish cache lines or resume a real peer.
//!
//! Only exclusively owned ordinary bytes are accessed. Never manufacture a
//! slice from the received address. A live adapter must separately establish
//! buffer lifetime, peer quiescence, mapping, cache maintenance and publication.
//! Unlike the SDK callback, malformed input is rejected before TRNG access,
//! and a failed fill leaves the destination unchanged. Private caller-provided
//! staging avoids allocation and partial publication; it can contain random
//! bytes after success/failure and must not be shared with an active reader.

use super::{Message, MessageId};

const ALIAS_OFFSET: u32 = 0x1000_0000;

/// Registered whole buffer extent in the control core's local address space.
/// Numeric validation is not proof of physical RAM, ownership or access rights.
/// This is independent of the CT-SRAM-only mailbox placement range: a random
/// buffer can instead be allocated from the SDK control-core mux-RAM heap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct BufferRegion {
    start: u32,
    length: u32,
}
impl BufferRegion {
    /// Explicit exclusive-end bounds; reject empty/reversed/alias overflow.
    /// No default heap, address, firmware or MCU memory mode is assumed.
    pub const fn try_from_control_bounds(start: u32, end: u32) -> Option<Self> {
        if start >= end || end.checked_add(ALIAS_OFFSET).is_none() {
            None
        } else {
            Some(Self {
                start,
                length: end - start,
            })
        }
    }
    /// Control-local numeric address, never an application pointer.
    pub const fn control_start(self) -> u32 {
        self.start
    }
    /// Numeric application alias; does not grant a reference or live lease.
    pub const fn application_start(self) -> u32 {
        self.start + ALIAS_OFFSET
    }
    /// Whole registered byte count, not just the selected half.
    pub const fn byte_len(self) -> u32 {
        self.length
    }
}

/// SDK wire update type. Upper/lower denote address order, not entropy quality.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum UpdateKind {
    /// Replace the entire registered buffer (wire0).
    All,
    /// Replace its first half (wire1).
    FirstHalf,
    /// Replace its second half (wire2).
    SecondHalf,
}

/// No remote memory or TRNG access occurs when request decoding fails.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum RequestError {
    /// Only IPC18 may invoke this service.
    WrongMessage,
    /// Exactly three words are required; no trailing bytes are accepted.
    PayloadLength,
    /// SDK discriminant is not0/1/2.
    UnknownUpdate,
    /// Empty extent, local-end wrap or application-alias-end wrap.
    InvalidExtent,
    /// Entire request extent must exactly match the registered buffer.
    UnregisteredRegion,
    /// Odd whole lengths cannot be split into two equal SDK halves.
    UnevenHalves,
}

/// Validated request bound to one numeric registration, not a memory capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct UpdateRequest {
    region: BufferRegion,
    kind: UpdateKind,
}
impl UpdateRequest {
    /// Decode only a locally copied, framed message received from a separately
    /// qualified control-core route. Message ID alone does not authenticate it.
    /// The callback may not replace registration by trusting its own payload.
    pub fn decode(message: Message<'_>, registered: BufferRegion) -> Result<Self, RequestError> {
        if message.id() != MessageId::UPDATE_SHARED_RANDOM_NUMBER {
            return Err(RequestError::WrongMessage);
        }
        let p = message.payload();
        if p.len() != 12 {
            return Err(RequestError::PayloadLength);
        }
        let word = |n| u32::from_le_bytes([p[n], p[n + 1], p[n + 2], p[n + 3]]);
        let kind = match word(0) {
            0 => UpdateKind::All,
            1 => UpdateKind::FirstHalf,
            2 => UpdateKind::SecondHalf,
            _ => return Err(RequestError::UnknownUpdate),
        };
        let start = word(4);
        let end = start
            .checked_add(word(8))
            .ok_or(RequestError::InvalidExtent)?;
        let region =
            BufferRegion::try_from_control_bounds(start, end).ok_or(RequestError::InvalidExtent)?;
        if region != registered {
            return Err(RequestError::UnregisteredRegion);
        }
        if kind != UpdateKind::All && !region.length.is_multiple_of(2) {
            return Err(RequestError::UnevenHalves);
        }
        Ok(Self { region, kind })
    }
    /// SDK whole/first-half/second-half selection.
    pub const fn kind(self) -> UpdateKind {
        self.kind
    }
    /// Offset relative to the registered buffer, not a bus address.
    pub const fn offset(self) -> usize {
        match self.kind {
            UpdateKind::All | UpdateKind::FirstHalf => 0,
            UpdateKind::SecondHalf => (self.region.length / 2) as usize,
        }
    }
    /// Selected bytes. Arbitrary tails use fresh TRNG bytes; odd half splits
    /// are rejected, rather than copying the SDK's inconsistent half lengths.
    pub const fn byte_len(self) -> usize {
        match self.kind {
            UpdateKind::All => self.region.length as usize,
            _ => (self.region.length / 2) as usize,
        }
    }
}

/// Dispatch/fill failure. No destination bytes change on any error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ServiceError<E> {
    /// Invalid or differently registered request.
    Request(RequestError),
    /// Private staging must hold the complete selected region before filling.
    ScratchTooSmall,
    /// Source failure; its written prefix refers to staging, not destination.
    Random(E),
}

/// Exclusive local image of a registered shared-random buffer.
/// It neither maps nor writes the wire address. A real, concurrently accessible
/// shared allocation must not be passed as `&mut [u8]` without first obtaining
/// exclusive ownership and completing any required cache handoff.
pub struct RandomBufferImage<'a> {
    region: BufferRegion,
    bytes: &'a mut [u8],
}
impl<'a> RandomBufferImage<'a> {
    /// Bind ordinary exclusive bytes to exactly one pre-registered extent.
    /// Length mismatch fails without changing bytes; address is metadata only.
    pub fn attach(region: BufferRegion, bytes: &'a mut [u8]) -> Option<Self> {
        (bytes.len() == region.length as usize).then_some(Self { region, bytes })
    }
    /// Inspect the exclusively owned image; no peer publication is implied.
    pub fn bytes(&self) -> &[u8] {
        self.bytes
    }
    /// Fill the selected range transactionally using a caller-provided entropy
    /// operation. The platform supplies its own attempt budget and delay policy.
    /// No allocation, global interrupt masking, IPC ACK or notification occurs.
    /// On success only selected bytes change; unused staging tail is untouched.
    /// On random failure even the selected destination remains unchanged, but
    /// consumed entropy and the staged prefix cannot be rolled back.
    pub fn update<E>(
        &mut self,
        request: UpdateRequest,
        scratch: &mut [u8],
        fill: impl FnOnce(&mut [u8]) -> Result<(), E>,
    ) -> Result<(), ServiceError<E>> {
        self.check(request, scratch.len())?;
        self.stage_and_copy(request, scratch, fill)
            .map_err(ServiceError::Random)
    }
    fn check<E>(&self, request: UpdateRequest, scratch_len: usize) -> Result<(), ServiceError<E>> {
        if request.region != self.region {
            return Err(ServiceError::Request(RequestError::UnregisteredRegion));
        }
        if scratch_len < request.byte_len() {
            return Err(ServiceError::ScratchTooSmall);
        }
        Ok(())
    }
    // Caller checks registration and staging extent before invoking this helper.
    fn stage_and_copy<E>(
        &mut self,
        request: UpdateRequest,
        scratch: &mut [u8],
        fill: impl FnOnce(&mut [u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        let n = request.byte_len();
        fill(&mut scratch[..n])?;
        self.bytes[request.offset()..request.offset() + n].copy_from_slice(&scratch[..n]);
        Ok(())
    }
}

#[cfg(all(test, not(target_arch = "riscv32")))]
#[path = "random_buffer_tests.rs"]
mod tests;
