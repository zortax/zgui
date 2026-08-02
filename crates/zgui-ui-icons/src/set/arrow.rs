//! The four arrows: a shaft and a solid head, in a 24-unit square.

use crate::IconData;

/// An arrow pointing up.
pub const ARROW_UP: IconData = IconData::new(
    "arrow-up",
    24.0,
    "M11 21 L13 21 L13 8.8 L17.6 13.4 L19 12 L12 5 L5 12 L6.4 13.4 L11 8.8 Z",
);

/// An arrow pointing down.
pub const ARROW_DOWN: IconData = IconData::new(
    "arrow-down",
    24.0,
    "M13 3 L11 3 L11 15.2 L6.4 10.6 L5 12 L12 19 L19 12 L17.6 10.6 L13 15.2 Z",
);

/// An arrow pointing left.
pub const ARROW_LEFT: IconData = IconData::new(
    "arrow-left",
    24.0,
    "M21 13 L21 11 L8.8 11 L13.4 6.4 L12 5 L5 12 L12 19 L13.4 17.6 L8.8 13 Z",
);

/// An arrow pointing right.
pub const ARROW_RIGHT: IconData = IconData::new(
    "arrow-right",
    24.0,
    "M3 13 L3 11 L15.2 11 L10.6 6.4 L12 5 L19 12 L12 19 L10.6 17.6 L15.2 13 Z",
);
