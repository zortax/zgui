//! The four chevrons: a bent ribbon, in a 24-unit square.
//!
//! A chevron is what a disclosure, a select and a pagination control point with, and the four are
//! the same shape at four quarter turns.

use crate::IconData;

/// A chevron pointing up.
pub const CHEVRON_UP: IconData = IconData::new(
    "chevron-up",
    24.0,
    "M12 8.4 L19.1 15.5 L17.7 16.9 L12 11.2 L6.3 16.9 L4.9 15.5 Z",
);

/// A chevron pointing down.
pub const CHEVRON_DOWN: IconData = IconData::new(
    "chevron-down",
    24.0,
    "M12 15.6 L4.9 8.5 L6.3 7.1 L12 12.8 L17.7 7.1 L19.1 8.5 Z",
);

/// A chevron pointing left.
pub const CHEVRON_LEFT: IconData = IconData::new(
    "chevron-left",
    24.0,
    "M8.4 12 L15.5 4.9 L16.9 6.3 L11.2 12 L16.9 17.7 L15.5 19.1 Z",
);

/// A chevron pointing right.
pub const CHEVRON_RIGHT: IconData = IconData::new(
    "chevron-right",
    24.0,
    "M15.6 12 L8.5 19.1 L7.1 17.7 L12.8 12 L7.1 6.3 L8.5 4.9 Z",
);
