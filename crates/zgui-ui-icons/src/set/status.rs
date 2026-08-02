//! The status marks: a ring or a triangle with a glyph inside it.
//!
//! Each is one outline. The ring is an outer circle wound one way and an inner circle wound the
//! other, so filling it with the non-zero rule leaves the middle empty; the glyph inside is a
//! solid subpath that touches neither.

use crate::IconData;

/// The ring the circled marks are drawn in, and the trailing space that separates it from what
/// follows.
macro_rules! circle_ring {
    () => {
        "M12 3 C16.971 3 21 7.029 21 12 C21 16.971 16.971 21 12 21 \
         C7.029 21 3 16.971 3 12 C3 7.029 7.029 3 12 3 Z \
         M12 5 C8.134 5 5 8.134 5 12 C5 15.866 8.134 19 12 19 \
         C15.866 19 19 15.866 19 12 C19 8.134 15.866 5 12 5 Z "
    };
}

/// The triangle the warning mark is drawn in, likewise hollow.
macro_rules! triangle_ring {
    () => {
        "M12 2.5 L22.5 20.5 L1.5 20.5 Z M12 6.8 L5.1 18.7 L18.9 18.7 Z "
    };
}

/// An exclamation mark inside a ring: something the reader has to know.
pub const ALERT_CIRCLE: IconData = IconData::new(
    "alert-circle",
    24.0,
    concat!(
        circle_ring!(),
        "M11 7 L13 7 L13 13 L11 13 Z ",
        "M11 15 L13 15 L13 17 L11 17 Z"
    ),
);

/// An exclamation mark inside a triangle: something that will go wrong.
pub const ALERT_TRIANGLE: IconData = IconData::new(
    "alert-triangle",
    24.0,
    concat!(
        triangle_ring!(),
        "M11 10 L13 10 L13 14.5 L11 14.5 Z ",
        "M11 15.8 L13 15.8 L13 17.8 L11 17.8 Z"
    ),
);

/// A lower-case `i` inside a ring: something worth knowing.
pub const INFO: IconData = IconData::new(
    "info",
    24.0,
    concat!(
        circle_ring!(),
        "M11 6.8 L13 6.8 L13 8.8 L11 8.8 Z ",
        "M11 10.5 L13 10.5 L13 17 L11 17 Z"
    ),
);

/// A tick inside a ring: something that went right.
pub const CHECK_CIRCLE: IconData = IconData::new(
    "check-circle",
    24.0,
    concat!(
        circle_ring!(),
        "M16.42 9.45 L15.59 8.62 L10.75 13.46 L8.41 11.12 L7.58 11.95 L10.75 15.12 Z"
    ),
);

/// A cross inside a ring: something that did not.
pub const CROSS_CIRCLE: IconData = IconData::new(
    "cross-circle",
    24.0,
    concat!(
        circle_ring!(),
        "M15.52 9.25 L14.75 8.48 L12 11.23 L9.25 8.48 L8.48 9.25 L11.23 12 \
         L8.48 14.75 L9.25 15.52 L12 12.77 L14.75 15.52 L15.52 14.75 L12.77 12 Z"
    ),
);
