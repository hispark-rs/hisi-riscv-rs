//! Explicit local session invalidation; this does not detect peer reset.
use crate::endpoint::{Endpoint, ReceiveError, SendError};

/// Owns one endpoint and stops all use after explicit invalidation.
///
/// A platform must invalidate this value when it observes or otherwise knows
/// that the peer or shared memory was reset/rebuilt. This wrapper does not
/// detect an unobservable reset, quiesce hardware, or reclaim shared memory.
pub struct Session<E> {
    endpoint: E,
    valid: bool,
}
impl<E> Session<E> {
    pub fn new(endpoint: E) -> Self {
        Self {
            endpoint,
            valid: true,
        }
    }
    pub fn invalidate(&mut self) {
        self.valid = false;
    }
    pub fn is_valid(&self) -> bool {
        self.valid
    }
}
impl<E: Endpoint> Endpoint for Session<E> {
    fn try_send(&mut self, message: &[u8]) -> Result<(), SendError> {
        if !self.valid {
            return Err(SendError::Invalidated);
        }
        self.endpoint.try_send(message)
    }
    fn try_receive(&mut self, output: &mut [u8]) -> Result<Option<usize>, ReceiveError> {
        if !self.valid {
            return Err(ReceiveError::Invalidated);
        }
        self.endpoint.try_receive(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Host-only bounded endpoint; no hardware or cross-core claim.
    struct Slot {
        data: [u8; 4],
        len: Option<usize>,
    }
    impl Endpoint for Slot {
        fn try_send(&mut self, bytes: &[u8]) -> Result<(), SendError> {
            if bytes.len() > self.data.len() {
                return Err(SendError::TooLong {
                    maximum: self.data.len(),
                });
            }
            if self.len.is_some() {
                return Err(SendError::Full);
            }
            self.data[..bytes.len()].copy_from_slice(bytes);
            self.len = Some(bytes.len());
            Ok(())
        }
        fn try_receive(&mut self, out: &mut [u8]) -> Result<Option<usize>, ReceiveError> {
            let Some(n) = self.len else {
                return Ok(None);
            };
            if out.len() < n {
                return Err(ReceiveError::BufferTooSmall { required: n });
            }
            out[..n].copy_from_slice(&self.data[..n]);
            self.len = None;
            Ok(Some(n))
        }
    }
    #[test]
    fn bounded_delivery_preserves_message_and_invalidation_stops_access() {
        let mut s = Session::new(Slot {
            data: [0; 4],
            len: None,
        });
        assert_eq!(s.try_receive(&mut []), Ok(None));
        assert_eq!(
            s.try_send(&[1, 2, 3, 4, 5]),
            Err(SendError::TooLong { maximum: 4 })
        );
        assert_eq!(s.try_send(&[1, 2, 3]), Ok(()));
        assert_eq!(s.try_send(&[9]), Err(SendError::Full));
        let mut short = [0xaa; 2];
        assert_eq!(
            s.try_receive(&mut short),
            Err(ReceiveError::BufferTooSmall { required: 3 })
        );
        assert_eq!(short, [0xaa; 2]);
        let mut out = [0; 4];
        assert_eq!(s.try_receive(&mut out), Ok(Some(3)));
        assert_eq!(&out[..3], &[1, 2, 3]);
        s.invalidate();
        assert_eq!(s.try_send(&[]), Err(SendError::Invalidated));
        assert_eq!(s.try_receive(&mut out), Err(ReceiveError::Invalidated));
        assert!(!s.is_valid());
    }
}
