//! The keyboard and the pointer, limited to a set of surfaces.

/// The name of one focus grab, for as long as it exists.
///
/// A focus grab limits the keyboard and the pointer to a set of this application's surfaces. A
/// press on any other surface, this application's own included, clears the grab. The platform
/// then wakes the loop with [`WakeReason::FocusGrabCleared`](crate::WakeReason::FocusGrabCleared).
///
/// Inside a grab the keyboard follows the pointer between the surfaces of the set. A surface the
/// keyboard must not reach while the pointer is over it stays out of the set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FocusGrabId(u64);

impl FocusGrabId {
    /// The identifier a backend gives the grab it knows as `raw`.
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The number behind the identifier.
    pub const fn get(self) -> u64 {
        self.0
    }
}
