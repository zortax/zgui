//! What a scheduled callback is called.

/// One scheduled callback, as the host named it.
///
/// A view holds one only long enough to cancel the callback again; [`set_timeout`] and
/// [`set_interval`] wrap it in a handle that cancels on drop, which is what a component uses.
///
/// [`set_timeout`]: crate::time::set_timeout
/// [`set_interval`]: crate::time::set_interval
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
pub struct TimerId(u64);

impl TimerId {
    /// Wraps a host's own number.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The host's own number.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Whether a scheduled callback runs once or keeps running.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum Repeat {
    /// Run once, then forget the registration.
    Once,
    /// Run every time the interval elapses, until cancelled.
    Every,
}

impl Repeat {
    /// Whether a callback scheduled this way runs more than once.
    pub const fn is_repeating(self) -> bool {
        matches!(self, Self::Every)
    }
}
