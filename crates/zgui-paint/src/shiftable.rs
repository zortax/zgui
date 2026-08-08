//! Whether a scrollport's pixels may be moved instead of drawn again.
//!
//! A renderer that keeps its composed frame can answer a scroll by translating the part of the port
//! that is still valid. What it cannot do is decide whether that is the same picture, because the
//! two things that would make it a different picture are properties of the document:
//!
//! * **something else draws in the port.** A tooltip over a list, a dialog, a sibling that overlaps
//!   it — the composed pixels are the composite, so moving them moves whatever was composited in.
//! * **the port is not opaque.** The composite includes whatever showed through the content, and
//!   what showed through does not scroll. Moving it smears it.
//!
//! Both are refusals rather than corrections. The fallback is the frame the window would have drawn
//! anyway, so a refusal costs the optimisation and never the picture.
//!
//! # What is not tested here, because it cannot happen
//!
//! That the movement was rigid. A subtree containing a sticky box, a viewport-anchored box or a
//! transform has `subtree_rigid` false, so the fragment pass composes it instead of offsetting it
//! and no move is reported at all — the layout tree's fragment diff decides that in
//! `can_translate`. By the time anything asks this question, the only
//! subtrees that moved are ones every piece of which moved by the same vector.

use zgui_css::ComputedStyle;
use zgui_css::values::image::ImageValue;
use zgui_dom::NodeKey;
use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Rect};
use zgui_layout::FragmentKind;
use zgui_layout::LayoutStore;
use zgui_layout::scroll_region;

use crate::walk::stacking::children_in_paint_order;

/// Why a port's pixels may not be moved.
///
/// Reported rather than folded into a bool so that a counter can say which refusal a document keeps
/// hitting: "scrolling is not being shifted" and "scrolling is not being shifted *because the list
/// has no background*" are different bugs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The element does not generate a box, or generates no scrollable region.
    NotAScroller,
    /// Its background lets what is behind it through, so the composite is not the content's alone.
    NotOpaque,
    /// Something outside the container draws inside the port.
    Overdrawn,
}

/// Whether `container`'s scrollport may have its pixels moved, and the port if it may.
///
/// The port is in device pixels, in the same space the damage set is measured in.
pub fn port_may_be_shifted(
    store: &LayoutStore,
    container: NodeKey,
) -> Result<Rect<DevicePx, Device>, Refusal> {
    let Some(&box_key) = store.boxes_of(container).first() else {
        return Err(Refusal::NotAScroller);
    };
    let Some(region) = scroll_region::region_of(store, box_key) else {
        return Err(Refusal::NotAScroller);
    };
    let port = region.scrollport;
    // Two independent conditions, and neither implies the other. A port with an opaque background
    // of its own still cannot be moved if a dialog is drawn over it, and a port with nothing over
    // it still cannot be moved if what shows through it is a gradient.
    if !only_this_box_draws_in(store, box_key, port) {
        return Err(Refusal::Overdrawn);
    }
    backing(store, box_key, port)?;
    Ok(port)
}

/// Whether nothing painted *after* `box_key` puts ink inside `port`.
///
/// Walks from the box up to the root, and at each level asks which of that level's children paint
/// after the one leading down to the container. Only those can be over it; anything earlier is
/// behind it and is [`backing`]'s question rather than this one's.
///
/// A later sibling whose whole folded subtree misses the port is dismissed without being entered,
/// so the cost is the container's depth and the fan-out along it rather than the document.
fn only_this_box_draws_in(
    store: &LayoutStore,
    box_key: BoxKey,
    port: Rect<DevicePx, Device>,
) -> bool {
    let Some(root) = store.root() else {
        return false;
    };
    let mut inside = box_key;
    while inside != root {
        let Some(parent) = store.get(inside).and_then(|node| node.parent) else {
            return false;
        };
        let order = children_in_paint_order(store, parent);
        let Some(position) = order.iter().position(|&child| child == inside) else {
            return false;
        };
        for &later in &order[position + 1..] {
            if paints_in(store, later, port) {
                return false;
            }
        }
        inside = parent;
    }
    true
}

/// Whether what the port is composited over is a uniform colour that nothing draws across.
///
/// The composed pixels inside a scrollport are the composite of everything under it, and moving
/// them moves all of it. That is the same picture on one condition: everything the content was
/// composited *over* is one flat colour, so translating it is the identity, and nothing was
/// composited *between* that colour and the content.
///
/// It is deliberately not "the scroll container has a background". A list usually does not — it
/// inherits the page's — and requiring one would refuse the ordinary case while accepting a
/// contrived one. What is required is that somewhere at or above the container there is an opaque
/// solid colour covering the port, and that between it and the content nothing else marks a pixel
/// inside the port.
///
/// The walk is up the ancestors, and at every level it is the same three questions, so it costs the
/// container's depth and the fan-out along it — never the document.
fn backing(
    store: &LayoutStore,
    box_key: BoxKey,
    port: Rect<DevicePx, Device>,
) -> Result<(), Refusal> {
    let Some(root) = store.root() else {
        return Err(Refusal::NotAScroller);
    };

    // The container's own background comes first: one that is opaque and flat hides everything
    // behind it, and then nothing above matters at all.
    if flat_and_opaque_over(store, box_key, port) {
        return Ok(());
    }

    let mut inside = box_key;
    loop {
        let Some(parent) = store.get(inside).and_then(|node| node.parent) else {
            // Not under the root this frame, which is not a document anything should be moving
            // pixels for.
            return Err(Refusal::Overdrawn);
        };

        // Every sibling at this level that paints *before* the one leading down to the container.
        // It is painted over whatever backs it and under the content, so it is between them, and it
        // does not scroll. What paints after is the other condition's to refuse.
        let order = children_in_paint_order(store, parent);
        let position = order.iter().position(|&child| child == inside);
        for &sibling in &order[..position.unwrap_or(0)] {
            if paints_in(store, sibling, port) {
                return Err(Refusal::Overdrawn);
            }
        }

        // The parent's own painting is behind everything at this level. Flat and opaque over the
        // whole port, it *is* the backing and the walk is done.
        if flat_and_opaque_over(store, parent, port) {
            return Ok(());
        }
        // Anything else it marks inside the port is between whatever backs it and the content.
        if marks_pixels_in(store, parent, port) {
            return Err(Refusal::NotOpaque);
        }
        if parent == root {
            // Nothing at or below the root declared a colour for these pixels. What is actually
            // behind them is whatever the surface was last cleared to, which is not this walk's to
            // know.
            return Err(Refusal::NotOpaque);
        }
        inside = parent;
    }
}

/// Whether `key` paints one flat opaque colour across the whole of `port`.
///
/// Flat because a gradient or an image is not the same picture when it is translated, and opaque
/// because anything less lets what is behind it into the composite. Across the whole port, because
/// a colour that covers half of it backs half of it.
fn flat_and_opaque_over(store: &LayoutStore, key: BoxKey, port: Rect<DevicePx, Device>) -> bool {
    let Some(style) = store.get(key).map(|node| &node.style) else {
        return false;
    };
    if !paints_opaquely(style) {
        return false;
    }
    // The background is painted over the border box, so that is what has to contain the port.
    store
        .fragments_of_box(key)
        .first()
        .and_then(|frag| store.fragment(*frag))
        .is_some_and(|piece| piece.border_box.intersection(port) == Some(port))
}

/// Whether a box's own background hides everything behind it, with one flat colour.
///
/// A solid, fully opaque colour is the one case where moving a composite is exactly moving the
/// content drawn on it: the colour is uniform, so translating it is the identity, and nothing
/// behind it contributed a pixel. A gradient or an image is not uniform and would smear; a
/// translucent colour lets what is behind it through, and what is behind it did not scroll.
fn paints_opaquely(style: &ComputedStyle) -> bool {
    let background = style.get_background();
    // Not `is_empty`: the initial value of `background-image` is a one-element list holding `none`,
    // so an emptiness test refuses every box in every document.
    if background
        .background_image
        .0
        .iter()
        .any(|image| !matches!(image, ImageValue::None))
    {
        return false;
    }
    // `currentColor` resolves against the box's own colour, which is already absolute here.
    let colour = background
        .background_color
        .resolve_to_absolute(&style.get_inherited_text().clone_color());
    colour.alpha >= 1.0
}

/// Whether `key`'s own fragments mark a pixel inside `port`, ignoring its descendants.
fn marks_pixels_in(store: &LayoutStore, key: BoxKey, port: Rect<DevicePx, Device>) -> bool {
    store.fragments_of_box(key).iter().any(|&frag| {
        store.fragment(frag).is_some_and(|piece| {
            piece.ink.intersection(port).is_some() && marks_pixels(store, key, piece.kind)
        })
    })
}

/// Whether anything in `key`'s subtree actually marks a pixel inside `port`.
///
/// The folded subtree ink is the *refusal* test and not the answer: a container whose descendants
/// straddle the port but whose painted boxes all avoid it marks nothing there, and taking the union
/// as the answer refuses every document with a full-height sibling in it. So the union is used the
/// way the emit walk uses it — to dismiss a whole subtree in constant time — and the question is
/// then asked of each box's own ink.
fn paints_in(store: &LayoutStore, key: BoxKey, port: Rect<DevicePx, Device>) -> bool {
    let Some(folded) = subtree_ink(store, key) else {
        return false;
    };
    if folded.intersection(port).is_none() {
        return false;
    }
    for &frag in store.fragments_of_box(key) {
        let Some(piece) = store.fragment(frag) else {
            continue;
        };
        if piece.ink.intersection(port).is_none() {
            continue;
        }
        if marks_pixels(store, key, piece.kind) {
            return true;
        }
    }
    store.get(key).is_some_and(|node| {
        node.children
            .iter()
            .any(|&child| paints_in(store, child, port))
    })
}

/// Whether a fragment of this box actually marks a pixel, rather than merely occupying a rectangle.
///
/// A fragment's ink is its geometric extent, so a full-height wrapper with no background "inks" the
/// whole window while drawing nothing at all. Asking the extent alone therefore refuses every
/// document with a layout wrapper in it, which is every document.
///
/// The lowerings are the paint stage's own, so this cannot drift from what is actually drawn: if
/// something new starts marking pixels, it starts marking them here too.
fn marks_pixels(store: &LayoutStore, key: BoxKey, kind: FragmentKind) -> bool {
    // Anything that is not a plain box is content: a run of text, a replaced element, a drawing, a
    // scrollbar. All of them draw.
    if !matches!(kind, FragmentKind::Box) {
        return true;
    }
    let Some(style) = store.get(key).map(|node| &node.style) else {
        return false;
    };
    let background = crate::lower::background::of(style);
    if background.color.alpha() > 0.0 || !background.layers.is_empty() {
        return true;
    }
    if !crate::lower::border::of(style).invisible {
        return true;
    }
    if crate::lower::outline::of(style, 1.0).is_some() {
        return true;
    }
    !style.get_effects().box_shadow.0.is_empty()
}

/// The union of a box's ink and everything below it, as the last fragment pass folded it.
fn subtree_ink(store: &LayoutStore, key: BoxKey) -> Option<Rect<DevicePx, Device>> {
    let frag = *store.fragments_of_box(key).first()?;
    store.fragment(frag).map(|fragment| fragment.subtree_ink)
}
