//! What a requested frame callback is called.

/// One pending frame callback, as the host named it.
///
/// A view holds one only long enough to cancel the callback again; [`request_frame`] wraps it in
/// a handle that cancels on drop, which is what a component uses.
///
/// [`request_frame`]: crate::time::request_frame
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
pub struct FrameRequestId(u64);

impl FrameRequestId {
    /// Wraps a host's own number.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The host's own number.
    pub const fn get(self) -> u64 {
        self.0
    }
}
