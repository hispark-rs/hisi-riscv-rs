//! Chip-neutral bounded message transport contract.
//!
//! A successful send means publication to the transport, not peer receipt or
//! processing. `Indeterminate` means a message may have been published: callers
//! must not blindly retry a non-idempotent operation. A short receive buffer
//! leaves the message available and reports the required capacity. Corruption
//! is not automatically repaired and must not be treated as an empty queue.

/// A message has not been published unless the error says otherwise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SendError {
    /// Backpressure; the message was not published.
    Full,
    /// The message exceeds the endpoint's supported bound; not published.
    TooLong { maximum: usize },
    /// Transport state is corrupted; not published by this call.
    Corrupt,
    /// The session is no longer usable; not published by this call.
    Invalidated,
    /// Publication may have happened; the caller must not assume retry is safe.
    Indeterminate,
}

/// Receive failure; all variants leave the pending message unconsumed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiveError {
    /// A message is pending but does not fit; retry with at least this capacity.
    BufferTooSmall { required: usize },
    /// The message or transport state is corrupt. No implicit queue reset occurs.
    Corrupt,
    /// The session is no longer usable.
    Invalidated,
}

/// Nonblocking, allocation-free endpoint over opaque complete messages.
///
/// `Ok(None)` means no message is currently available. Implementations must
/// serialize each local producer/consumer and preserve the error guarantees
/// above. Neither this trait nor a successful call proves peer ownership,
/// peer processing, cache coherency, or delivery after a reset. Async/wakeup
/// behavior is not specified by this synchronous contract.
pub trait Endpoint {
    fn try_send(&mut self, message: &[u8]) -> Result<(), SendError>;
    fn try_receive(&mut self, output: &mut [u8]) -> Result<Option<usize>, ReceiveError>;
}
