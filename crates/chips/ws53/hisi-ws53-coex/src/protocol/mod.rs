//! Explicit firmware-protocol selection. Selection itself does not identify
//! the running control image or prove wire compatibility.

/// The SDK source profile whose message and queue behavior was examined.
/// The actual peer must be verified separately by the platform integrator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FirmwareProtocol {
    /// SDK commit 163d9150, application-side IPC profile. The retained control
    /// firmware has not been automatically identified by this library.
    Sdk163d9150,
}

/// Codecs and mailbox layout for the explicitly selected SDK profile.
pub mod sdk_163d9150 {
    pub use crate::{MAX_MESSAGE, MAX_PAYLOAD, Message, MessageError, MessageId, Priority};
    pub use crate::{layout, mailbox};
}
