//! What is being dragged, and how far a press travels before it counts as a drag.

use zgui_dom::NodeKey;
use zgui_geom::{Css, CssPx, Point};

/// How far a pointer travels, in CSS pixels, before a press becomes a drag.
///
/// Smaller than the touch slop and for a different reason: a mouse is precise, so the threshold is
/// there to separate a click whose hand shook from a deliberate movement, not to accommodate a
/// contact patch. Too small and every click on a draggable row starts a drag; too large and the
/// drag feels stuck to the surface for the first centimetre.
pub const THRESHOLD: f32 = 4.0;

/// Whether a pointer that pressed at `from` and is now at `to` has passed the drag threshold.
pub fn past_threshold(from: Point<CssPx, Css>, to: Point<CssPx, Css>) -> bool {
    let dx = to.x.0 - from.x.0;
    let dy = to.y.0 - from.y.0;
    dx * dx + dy * dy > THRESHOLD * THRESHOLD
}

/// What a drag is carrying.
///
/// A node rather than a payload of bytes, because an internal drag never leaves the process: the
/// thing being dragged is an element of this document, and whatever it *means* is a question for
/// the application that put it there. Serialising it into a media type would be inventing an
/// exchange format for a hand-off that never crosses a boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DragSource {
    /// The element the drag started from.
    node: NodeKey,
}

impl DragSource {
    /// A drag carrying `node`.
    pub const fn node(node: NodeKey) -> Self {
        Self { node }
    }

    /// The element being dragged.
    pub const fn key(self) -> NodeKey {
        self.node
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Css, CssPx, Point};

    use super::{THRESHOLD, past_threshold};

    fn at(x: f32, y: f32) -> Point<CssPx, Css> {
        Point::new(CssPx(x), CssPx(y))
    }

    #[test]
    fn a_click_whose_hand_shook_is_still_a_click() {
        assert!(!past_threshold(at(0.0, 0.0), at(1.0, 1.0)));
        assert!(!past_threshold(at(0.0, 0.0), at(0.0, THRESHOLD)));
    }

    #[test]
    fn a_deliberate_movement_is_a_drag() {
        assert!(past_threshold(at(0.0, 0.0), at(0.0, THRESHOLD + 1.0)));
        assert!(past_threshold(at(20.0, 20.0), at(0.0, 20.0)));
    }

    #[test]
    fn the_threshold_is_below_the_touch_slop() {
        // A mouse is precise and a finger is not, so the distance that separates a click from a
        // drag must be smaller than the one that separates a tap from a scroll. Equal or larger and
        // a mouse drag would need more travel than a touch scroll to start.
        const {
            assert!(THRESHOLD < crate::gesture::SLOP);
        }
    }
}
