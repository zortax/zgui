//! What a registered listener is called.

/// One listener registration, as the backend named it.
///
/// A view holds one only long enough to remove the registration again. The number means whatever
/// the backend wants it to mean; two ids from the same backend are equal exactly when they name
/// the same registration.
///
/// ```
/// use zgui_view::ListenerId;
///
/// let first = ListenerId::new(1);
/// assert_eq!(first, ListenerId::new(1));
/// assert_ne!(first, ListenerId::new(2));
/// assert_eq!(first.get(), 1);
/// ```
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
pub struct ListenerId(u64);

impl ListenerId {
    /// Wraps a backend's own number.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The backend's own number.
    pub const fn get(self) -> u64 {
        self.0
    }
}
