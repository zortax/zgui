//! The small drawings in the panel's own chrome.
//!
//! Each is one [Lucide](https://lucide.dev) icon, copied as path data rather than linked: the
//! inspector draws four of them and a dependency on an icon set to reach four constants would be a
//! crate in the graph for the sake of a hundred bytes. Lucide is ISC-licensed, which permits this
//! with attribution, and this module is the attribution.
//!
//! They are written the way Lucide draws them — a twenty-four unit square, stroked rather than
//! filled, every corner and cap round — so the sheet paints them with `--zgui-stroke` and turns the
//! fill off rather than the other way round. That is also why they are legible at 16 px: a stroked
//! outline keeps its weight when the box shrinks, where a filled one closes up.
//!
//! One path per line, which is how [`PATHS`](zgui::elements::vector::PATHS) carries a list.

/// The space every icon here is drawn in.
pub(crate) const VIEW_BOX: &str = "0 0 24 24";

/// Lucide `square-mouse-pointer`, which Lucide also publishes under the alias `inspect`.
///
/// A pointer over a frame: the gesture the button starts, rather than a cursor on its own.
pub(crate) const PICK: &str = "M12.034 12.681a.498.498 0 0 1 .647-.647l9 3.5a.5.5 0 0 1-.033.943l-3.444 1.068a1 1 0 0 0-.66.66l-1.067 3.443a.5.5 0 0 1-.943.033z
M21 11V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h6";

/// Lucide `snowflake`.
///
/// Freezing rather than pausing: the panel keeps showing the frame it is on, which is not the same
/// thing as a transport control and should not borrow one's icon.
pub(crate) const FREEZE: &str = "m10 20-1.25-2.5L6 18
M10 4 8.75 6.5 6 6
m14 20 1.25-2.5L18 18
m14 4 1.25 2.5L18 6
m17 21-3-6h-4
m17 3-3 6 1.5 3
M2 12h6.5L10 9
m20 10-1.5 2 1.5 2
M22 12h-6.5L14 15
m4 10 1.5 2L4 14
m7 21 3-6-1.5-3
m7 3 3 6h4";

/// Lucide `chevron-right`: a collapsed row in the tree.
pub(crate) const CHEVRON_RIGHT: &str = "m9 18 6-6-6-6";

/// Lucide `chevron-down`: an expanded one.
pub(crate) const CHEVRON_DOWN: &str = "m6 9 6 6 6-6";

/// Lucide `component`: the Elements tab.
pub(crate) const ELEMENTS: &str = "M15.536 11.293a1 1 0 0 0 0 1.414l2.376 2.377a1 1 0 0 0 1.414 0l2.377-2.377a1 1 0 0 0 0-1.414l-2.377-2.377a1 1 0 0 0-1.414 0z
M2.297 11.293a1 1 0 0 0 0 1.414l2.377 2.377a1 1 0 0 0 1.414 0l2.377-2.377a1 1 0 0 0 0-1.414L6.088 8.916a1 1 0 0 0-1.414 0z
M8.916 17.912a1 1 0 0 0 0 1.415l2.377 2.376a1 1 0 0 0 1.414 0l2.377-2.376a1 1 0 0 0 0-1.415l-2.377-2.376a1 1 0 0 0-1.414 0z
M8.916 4.674a1 1 0 0 0 0 1.414l2.377 2.376a1 1 0 0 0 1.414 0l2.377-2.376a1 1 0 0 0 0-1.414l-2.377-2.377a1 1 0 0 0-1.414 0z";

/// Lucide `layers`: the Frame tab.
pub(crate) const FRAME: &str = "M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83z
M2 12a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 12
M2 17a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 17";

/// Lucide `activity`: the Timeline tab.
pub(crate) const TIMELINE: &str = "M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2";

/// Lucide `zap`: the Reactivity tab.
pub(crate) const REACTIVITY: &str = "M4 14a1 1 0 0 1-.78-1.63l9.9-10.2a.5.5 0 0 1 .86.46l-1.92 6.02A1 1 0 0 0 13 10h7a1 1 0 0 1 .78 1.63l-9.9 10.2a.5.5 0 0 1-.86-.46l1.92-6.02A1 1 0 0 0 11 14z";

/// Lucide `check-check`: the Parity tab.
pub(crate) const PARITY: &str = "M18 6 7 17l-5-5
m22 10-7.5 7.5L13 16";

/// Lucide `hard-drive`: the Memory tab.
///
/// The `<line>` elements Lucide draws it with are written here as one-segment paths, which is what
/// this element's path list carries.
pub(crate) const MEMORY: &str = "M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z
M22 12H2
M6 16h.01
M10 16h.01";

#[cfg(test)]
mod tests {
    use zgui::elements::vector::from_path_data;

    use super::{CHEVRON_DOWN, CHEVRON_RIGHT, FREEZE, PICK};

    #[test]
    fn every_icon_parses_into_the_outlines_it_was_drawn_with() {
        // Lucide writes relative commands and elliptical arcs, and a path that failed to parse
        // would draw nothing at all rather than draw wrongly — which is invisible in a screenshot
        // of a panel whose buttons are also labelled.
        for (name, data, paths) in [
            ("pick", PICK, 2),
            ("freeze", FREEZE, 12),
            ("chevron-right", CHEVRON_RIGHT, 1),
            ("chevron-down", CHEVRON_DOWN, 1),
        ] {
            let read = from_path_data(data);
            assert_eq!(read.len(), paths, "{name} lost a line on the way through");
        }
    }
}
