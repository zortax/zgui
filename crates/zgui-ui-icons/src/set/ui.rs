//! Icons an interface needs that are not marks or directions.

use crate::IconData;

/// A magnifying glass.
///
/// The lens is a hollow ring and the handle is a bar wound the same way as the ring's outside, so
/// the overlap between them stays filled instead of cancelling out.
pub const SEARCH: IconData = IconData::new(
    "search",
    24.0,
    "M10.5 3.5 C14.366 3.5 17.5 6.634 17.5 10.5 C17.5 14.366 14.366 17.5 10.5 17.5 \
     C6.634 17.5 3.5 14.366 3.5 10.5 C3.5 6.634 6.634 3.5 10.5 3.5 Z \
     M10.5 5.5 C7.739 5.5 5.5 7.739 5.5 10.5 C5.5 13.261 7.739 15.5 10.5 15.5 \
     C13.261 15.5 15.5 13.261 15.5 10.5 C15.5 7.739 13.261 5.5 10.5 5.5 Z \
     M14.15 15.85 L15.85 14.15 L21.65 19.95 L19.95 21.65 Z",
);

/// Three quarters of a ring, for something still happening.
///
/// One outline with a radial gap rather than two rings with a hole between them: the gap is what
/// makes a rotation visible, and a ring with no gap turning on the spot looks like a ring.
pub const SPINNER: IconData = IconData::new(
    "spinner",
    24.0,
    "M12 3 C16.971 3 21 7.029 21 12 C21 16.971 16.971 21 12 21 C7.029 21 3 16.971 3 12 \
     L5 12 C5 15.866 8.134 19 12 19 C15.866 19 19 15.866 19 12 C19 8.134 15.866 5 12 5 Z",
);

/// Three dots in a row, for a control that opens the rest of the options.
pub const ELLIPSIS: IconData = IconData::new(
    "ellipsis",
    24.0,
    "M5.5 10.1 C6.549 10.1 7.4 10.951 7.4 12 C7.4 13.049 6.549 13.9 5.5 13.9 \
     C4.451 13.9 3.6 13.049 3.6 12 C3.6 10.951 4.451 10.1 5.5 10.1 Z \
     M12 10.1 C13.049 10.1 13.9 10.951 13.9 12 C13.9 13.049 13.049 13.9 12 13.9 \
     C10.951 13.9 10.1 13.049 10.1 12 C10.1 10.951 10.951 10.1 12 10.1 Z \
     M18.5 10.1 C19.549 10.1 20.4 10.951 20.4 12 C20.4 13.049 19.549 13.9 18.5 13.9 \
     C17.451 13.9 16.6 13.049 16.6 12 C16.6 10.951 17.451 10.1 18.5 10.1 Z",
);

/// A rounded frame with a column ruled off down its left, for a panel beside a page.
///
/// The frame is an outline: the outer edge is wound one way and the inner edge the other, so what
/// is between them fills and what is inside the inner edge does not. The rule down the left is a
/// third subpath wound like the outer edge, which puts it back on the filled side.
pub const PANEL_LEFT: IconData = IconData::new(
    "panel-left",
    24.0,
    "M5 3 L19 3 C20.105 3 21 3.895 21 5 L21 19 C21 20.105 20.105 21 19 21 L5 21 \
     C3.895 21 3 20.105 3 19 L3 5 C3 3.895 3.895 3 5 3 Z \
     M6 5 C5.448 5 5 5.448 5 6 L5 18 C5 18.552 5.448 19 6 19 L18 19 \
     C18.552 19 19 18.552 19 18 L19 6 C19 5.448 18.552 5 18 5 Z \
     M8 5 L10 5 L10 19 L8 19 Z",
);
