//! Putting every box edge on a whole device pixel.
//!
//! The rule is *round the cumulative absolute edges, and derive each size as the difference between
//! two rounded edges*. Rounding each box's own size instead would let a column of ten boxes drift
//! by up to five pixels from the sum of its parts, and would leave one-pixel gaps between boxes
//! that share an edge.
//!
//! Rounding is not a pass of its own. It is arithmetic performed by the walk that composes absolute
//! positions, because that walk already has the cumulative origin rounding needs and running it
//! separately would compute the same origin twice — over *every* box, including the ones a frame
//! did not touch. What lives here is the arithmetic and the invariant it keeps; the walk that calls
//! it decides which boxes to visit.

use zgui_geom::{DevicePx, Edges};

/// The four absolute edges of one snapped box, in device pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapped {
    /// The left edge.
    pub left: f32,
    /// The top edge.
    pub top: f32,
    /// The right edge.
    pub right: f32,
    /// The bottom edge.
    pub bottom: f32,
}

/// Snaps one box sitting at `parent_layout` inside a parent whose own rounded origin is
/// `parent_rounded`.
///
/// Returns the layout result the box records — whose location is relative to its parent, exactly as
/// the layout algorithms report it — together with the absolute edges it was derived from, which is
/// what the box's own children are then snapped against.
pub fn place(
    unrounded: taffy::Layout,
    parent_layout: (f32, f32),
    parent_rounded: (f32, f32),
) -> (taffy::Layout, Snapped) {
    let x = parent_layout.0 + unrounded.location.x;
    let y = parent_layout.1 + unrounded.location.y;
    let absolute = Snapped {
        left: x.round(),
        top: y.round(),
        right: (x + unrounded.size.width).round(),
        bottom: (y + unrounded.size.height).round(),
    };

    let mut snapped = unrounded;
    snapped.location.x = absolute.left - parent_rounded.0;
    snapped.location.y = absolute.top - parent_rounded.1;
    snapped.size.width = absolute.right - absolute.left;
    snapped.size.height = absolute.bottom - absolute.top;
    snapped.border = round_edges(unrounded.border);
    snapped.padding = round_edges(unrounded.padding);
    snapped.content_size.width = (x + unrounded.content_size.width).round() - absolute.left;
    snapped.content_size.height = (y + unrounded.content_size.height).round() - absolute.top;
    (snapped, absolute)
}

/// The absolute unrounded origin one box sits at, walked up from it.
///
/// A subtree composed on its own still has to round against where it actually sits, so the seed is
/// read from the box's ancestors rather than assumed to be the page origin. Entering at a subtree
/// with a zero accumulator would round against the wrong origin and shift the whole subtree by up
/// to a pixel.
pub fn seed(store: &crate::tree::store::LayoutStore, key: zgui_dom::side::BoxKey) -> (f32, f32) {
    let mut x = 0.0;
    let mut y = 0.0;
    let mut next = store.get(key).and_then(|node| node.parent);
    while let Some(parent) = next {
        let Some(state) = store.state(parent) else {
            break;
        };
        x += state.unrounded.location.x;
        y += state.unrounded.location.y;
        next = store.get(parent).and_then(|node| node.parent);
    }
    (x, y)
}

/// Four insets in this framework's own unit, each rounded on its own.
///
/// Rounding them independently is right because each is measured from an edge that has itself been
/// rounded, so a rounded inset from a rounded edge lands on the grid.
pub fn edges(rect: taffy::Rect<f32>) -> Edges<DevicePx> {
    Edges {
        top: DevicePx(rect.top.round()),
        right: DevicePx(rect.right.round()),
        bottom: DevicePx(rect.bottom.round()),
        left: DevicePx(rect.left.round()),
    }
}

/// Rounds four insets, in the layout engine's own type.
fn round_edges(rect: taffy::Rect<f32>) -> taffy::Rect<f32> {
    taffy::Rect {
        left: rect.left.round(),
        right: rect.right.round(),
        top: rect.top.round(),
        bottom: rect.bottom.round(),
    }
}

#[cfg(test)]
mod tests {
    use super::place;

    /// A layout at one offset with one size, with no insets.
    fn layout(x: f32, y: f32, width: f32, height: f32) -> taffy::Layout {
        let mut layout = taffy::Layout::new();
        layout.location = taffy::Point { x, y };
        layout.size = taffy::Size { width, height };
        layout
    }

    #[test]
    fn a_size_is_the_difference_between_two_rounded_edges() {
        // Three boxes of 33.3 stacked from 0 must cover exactly 100 pixels with no gaps.
        let mut cursor = 0.0;
        let mut edges = Vec::new();
        for _ in 0..3 {
            let (_, snapped) = place(layout(cursor, 0.0, 33.4, 10.0), (0.0, 0.0), (0.0, 0.0));
            edges.push((snapped.left, snapped.right));
            cursor += 33.4;
        }
        for pair in edges.windows(2) {
            assert_eq!(
                pair[0].1, pair[1].0,
                "no gap and no overlap between {pair:?}"
            );
        }
        assert_eq!(edges[2].1, 100.0);
    }

    #[test]
    fn a_subtree_rounds_against_where_it_actually_sits() {
        // The same child, once against a parent at 0.5 and once against one at 0.0.
        let (against_half, _) = place(layout(0.7, 0.0, 10.0, 10.0), (0.5, 0.0), (1.0, 0.0));
        let (against_zero, _) = place(layout(0.7, 0.0, 10.0, 10.0), (0.0, 0.0), (0.0, 0.0));
        assert_ne!(
            against_half.location.x, against_zero.location.x,
            "seeding with zero would put the child in the wrong place"
        );
    }
}
