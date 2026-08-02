//! How one surface is told from another.

use core::fmt::{self, Debug};

/// Which surface something happened to.
///
/// Opaque and cheap: every event carries one, every wake that concerns a window names one, and
/// nothing above the platform layer needs to know how a backend numbers its own windows.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceId(u64);

impl SurfaceId {
    /// The identifier with the given raw value.
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw value, for a backend that has to map it back to its own handle.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl Debug for SurfaceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "SurfaceId({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::SurfaceId;

    #[test]
    fn identifiers_round_trip_and_compare() {
        assert_eq!(SurfaceId::new(9).raw(), 9);
        assert_ne!(SurfaceId::new(1), SurfaceId::new(2));
        assert_eq!(format!("{:?}", SurfaceId::new(4)), "SurfaceId(4)");
    }
}
