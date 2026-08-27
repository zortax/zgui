//! Where a box actually is on the screen, transforms and all.
//!
//! A fragment keeps its rectangle in its own untransformed space, because that is the space its
//! clip, its corner radii and its children are all expressed in. Everything *outside* layout that
//! asks where a box is means the other thing: the rectangle a person sees and a pointer lands on.
//! A surface placed against a trigger inside a transformed panel, a control measuring a gesture
//! against itself, and a fixture aiming at what it can see all want the same answer, and it is
//! this one — the same rectangle the paint stage draws into and the hit test resolves a pointer
//! against.

use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Rect};
use zgui_scene::{Placements, SpatialId};

use crate::fragment::FragKey;
use crate::fragment::transform::transformed_bounds;
use crate::tree::store::LayoutStore;

/// A rectangle measured in the coordinate system `space` names, put where it is drawn.
///
/// The one step between the two spaces every reader outside layout has to take, written once so
/// that a caret, an accessibility rectangle and an observed border box cannot each take it
/// slightly differently. A rectangle in a rotated or scaled space is not a rectangle any more, so
/// what comes back is the smallest upright box containing it.
///
/// A name the drawn frame no longer resolves leaves the rectangle where it is rather than
/// answering nothing: the alternative is a caller with no rectangle at all, which for a caret is
/// no insertion point and for a consumer outside this process is a control with no position.
pub fn onto_device(
    rect: Rect<DevicePx, Device>,
    space: Option<SpatialId>,
    placements: &Placements,
) -> Rect<DevicePx, Device> {
    match space.and_then(|id| placements.moves(id)) {
        Some(matrix) => transformed_bounds(matrix, rect),
        None => rect,
    }
}

/// The union of every fragment in `frags`, put where each of them is drawn.
///
/// `None` when the list is empty, which for a box or an element means it has not been composed.
pub fn placed_union(
    store: &LayoutStore,
    frags: &[FragKey],
    placements: &Placements,
) -> Option<Rect<DevicePx, Device>> {
    let mut union: Option<Rect<DevicePx, Device>> = None;
    for key in frags {
        let Some(fragment) = store.fragment(*key) else {
            continue;
        };
        let placed = onto_device(fragment.border_box, fragment.transform, placements);
        union = Some(match union {
            None => placed,
            Some(so_far) => so_far.union(placed),
        });
    }
    union
}

/// The rectangle `box_key`'s fragments occupy in the window, with every transform on and above
/// them applied.
///
/// The fragments rather than a walk up the tree summing origins: a fragment is the box after every
/// ancestor's origin has been accumulated and every ancestor's scroll offset taken off. A second
/// implementation of that accumulation would disagree with the first the moment either learned
/// about something the other did not — a `position: fixed` box, which takes no part of any
/// ancestor's scroll, is where the two part company first.
///
/// The matrix a fragment carries is the whole chain, its own transform and every ancestor's, so
/// one multiplication puts the rectangle where it is drawn. A transformed rectangle is not a
/// rectangle, so what comes back is the smallest upright box containing it — which for the
/// translations and scales interfaces are built out of is the rectangle itself.
///
/// `None` before the box has been composed at all, which is the state of a surface in the frame it
/// is first mounted in.
///
/// The matrices are the drawn frame's, resolved once when that frame finished composing. A
/// coordinate system is named by the box that establishes it, so the name a fragment carries goes
/// on meaning the same coordinate system between frames; what changes between frames is where that
/// coordinate system *is*, and this is the answer as of the picture on the screen.
pub fn window_box(
    store: &LayoutStore,
    box_key: BoxKey,
    placements: &Placements,
) -> Option<Rect<DevicePx, Device>> {
    placed_union(store, store.fragments_of_box(box_key), placements)
}
