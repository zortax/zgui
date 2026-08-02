//! The bars of a scrollport that is on the screen right now.
//!
//! Everything else in this module answers from numbers a caller already has. This answers from the
//! fragment tree, which is what a pointer needs: a press arrives between two frames, and the only
//! description of where the bar it landed on actually is, is the one the last frame drew.
//!
//! The offset is deliberately absent. Neither question a press asks — how far the thumb may travel,
//! and how long a screenful is — depends on where the container is currently scrolled to, and the
//! offset is not layout's to hold. The one caller that needs it holds it already.

use zgui_dom::NodeKey;
use zgui_geom::{Device, DevicePx, Point, Rect};
use zgui_scene::SpatialTree;

use crate::axis::Axis;
use crate::fragment::{FragmentKind, ScrollbarPart};
use crate::scroll_region::bar::Scrollport;
use crate::scroll_region::bar::travel::{self, Travel};
use crate::tree::store::LayoutStore;

/// The range one element's thumb moves in, as the last frame placed it.
///
/// Answers with nothing for an element that generated no box, was never laid out, or does not
/// scroll — all three of which are "there is no bar here", which is what a caller does about them.
pub fn travel_of(store: &LayoutStore, element: NodeKey, axis: Axis) -> Option<Travel> {
    let box_ = *store.boxes_of(element).first()?;
    let frag = *store.fragments_of_box(box_).first()?;
    let fragment = store.fragment(frag)?;
    let content = store.layout_of(box_)?.content_size;
    let port = Scrollport {
        inner: fragment.padding_box.inset(fragment.padding),
        content_box: fragment.content_box,
        content,
        offset: Point::new(DevicePx(0.0), DevicePx(0.0)),
    };
    port.reserves(axis).then(|| travel::of(&port, axis))
}

/// Where the thumb on `axis` was drawn, as the last frame placed it.
///
/// Read from the fragment rather than recomputed, so that what a press is measured against is what
/// is on the screen even in the frame where the two would disagree.
pub fn thumb_of(
    store: &LayoutStore,
    box_: crate::BoxKey,
    axis: Axis,
) -> Option<Rect<DevicePx, Device>> {
    store
        .fragments_of_box(box_)
        .iter()
        .filter_map(|frag| store.fragment(*frag))
        .find(|fragment| {
            fragment.kind
                == FragmentKind::Scrollbar {
                    axis,
                    part: ScrollbarPart::Thumb,
                }
        })
        .map(|fragment| fragment.border_box)
}

/// A device-space point, expressed in the space `box_`'s bars are measured in.
///
/// Every number a press on a bar is weighed against — the thumb's near edge from [`thumb_of`], the
/// ends of the track and the travel between them from [`travel_of`] — is in the box's own space,
/// because that is the space a fragment keeps its rectangle in. A pointer arrives in device pixels.
/// Under an identity chain the two are the same numbers, which is what makes mixing them invisible:
/// it is arithmetic on equal quantities until something at or above the scroller is scaled, shifted
/// or turned, and then a grab recorded as the pointer less the thumb is a difference of two
/// different spaces. The thumb leaves the pointer by exactly that difference on the first move and
/// tracks at the wrong rate after it.
///
/// So a press and every move that follows it are brought here first, and the whole of a drag is
/// arithmetic in one space rather than a correction applied to two.
///
/// Answers with nothing for a box that was never laid out, and for a space that collapses the
/// plane — a bar covering no pixels of the device is a bar no press can land on.
pub fn into_bar_space(
    store: &LayoutStore,
    spatial: &SpatialTree,
    box_: crate::BoxKey,
    point: Point<DevicePx, Device>,
) -> Option<Point<DevicePx, Device>> {
    let frag = *store.fragments_of_box(box_).first()?;
    let space = store.fragment(frag)?.transform;
    crate::fragment::hit::transform::into_local(point, space, spatial)
}
