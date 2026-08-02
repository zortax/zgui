//! The marks a control puts inside itself: a tick, a dash, a cross, a disc.
//!
//! These are the shapes a checkbox, a radio button and a dismiss button are made of, so they are
//! drawn to read at sixteen pixels rather than to look finished at ninety-six.

use crate::IconData;

/// A tick.
pub const CHECK: IconData = IconData::new(
    "check",
    24.0,
    "M20.5 7.1 L18.9 5.5 L9.6 14.8 L5.1 10.3 L3.5 11.9 L9.6 18.0 Z",
);

/// A dash, which is what a checkbox in its mixed state shows.
pub const MINUS: IconData = IconData::new("minus", 24.0, "M4 11 L20 11 L20 13 L4 13 Z");

/// A plus.
pub const PLUS: IconData = IconData::new(
    "plus",
    24.0,
    "M11 4 L13 4 L13 11 L20 11 L20 13 L13 13 L13 20 L11 20 L11 13 L4 13 L4 11 L11 11 Z",
);

/// A cross, which is what a dismiss control shows.
pub const CROSS: IconData = IconData::new(
    "cross",
    24.0,
    "M18.4 7.0 L17.0 5.6 L12 10.6 L7.0 5.6 L5.6 7.0 L10.6 12 L5.6 17.0 L7.0 18.4 L12 13.4 \
     L17.0 18.4 L18.4 17.0 L13.4 12 Z",
);

/// A filled disc, which is what a selected radio button shows.
pub const DISC: IconData = IconData::new(
    "disc",
    24.0,
    "M12 6 C15.314 6 18 8.686 18 12 C18 15.314 15.314 18 12 18 \
     C8.686 18 6 15.314 6 12 C6 8.686 8.686 6 12 6 Z",
);

/// A small filled dot, for a marker beside something else.
pub const DOT: IconData = IconData::new(
    "dot",
    24.0,
    "M12 8.5 C13.933 8.5 15.5 10.067 15.5 12 C15.5 13.933 13.933 15.5 12 15.5 \
     C10.067 15.5 8.5 13.933 8.5 12 C8.5 10.067 10.067 8.5 12 8.5 Z",
);
