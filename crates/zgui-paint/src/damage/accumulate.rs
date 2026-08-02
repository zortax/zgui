//! Growing the frame's damage before a single fragment is emitted.
//!
//! Two rectangles reach the damage set from here, and neither of them can come from anywhere else.
//!
//! # The composites that read outside what they write
//!
//! A `backdrop-filter` samples the composite *beneath* it, and a `filter: blur()` samples its own
//! target, in both cases over a region dilated well past the rectangle being written. The scene
//! texture is kept between frames and only the damaged rectangles are cleared and redrawn, so
//! outside a damage rectangle those reads land on the previous frame's final composite — which
//! already contains the group's own output. For a backdrop that is a feedback loop: a caret-sized
//! damage rectangle inside a frosted panel reads sixty pixels of the frame before it, and the panel
//! smears a little further every frame until the whole thing is fog. For a content filter it is not
//! a loop but the same missing region: a target populated only inside the damage rectangle blurs to
//! a faded edge.
//!
//! [`expand`] closes that, and *where* it runs is the whole point. It walks the read-extent registry
//! and never the fragment tree, because the emit walk's constant-time subtree skip is over a union
//! of *ink*, and a read extent is deliberately not in that union — so an expansion folded into the
//! emit walk would be dropped at an ancestor in exactly the case it exists for: content animating
//! *under* an untouched blurred dialog, whose whole ancestor chain misses the damage. And it runs
//! before the emit walk, because a rectangle added afterwards is cleared by the renderer and
//! repainted by nobody, which is a hole rather than a smear.
//!
//! # The pixels a removed subtree left behind
//!
//! [`vacated`] is the other one. What compares this frame's output against last frame's only ever
//! sees output that still exists, so the area a removed panel occupied is nobody's ink and nothing
//! downstream can recover it. The roots a frame removed are read from the document — their geometry
//! outlives the change and is discarded at the frame's recycling pass — and their subtree ink is
//! absorbed while it is still there to read.

use zgui_bits::DamageSet;
use zgui_dom::Document;
use zgui_geom::{Device, Size};
use zgui_layout::LayoutStore;
use zgui_layout::fragment::diff::pixels;

use crate::damage::ink::read_extent_of;

/// What one expansion pass did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Expansion {
    /// How many read extents were absorbed into the damage.
    pub absorbed: usize,
    /// How many passes over the registry were needed to reach a fixpoint.
    pub passes: usize,
    /// Whether the expansion gave up and damaged the whole surface.
    ///
    /// Two things cause it: a rectangle growing past roughly half the surface, at which point one
    /// full redraw is cheaper *and* simpler than a set of overlapping megarects; and the iteration
    /// bound being exhausted, which a pathological stack of mutually overlapping blurred panels
    /// could otherwise spin in every frame.
    pub escalated: bool,
}

/// The share of the surface above which a damage rectangle is not worth scissoring to.
///
/// Above roughly half, a full redraw costs less than the passes, the clears and the bookkeeping a
/// set of large overlapping rectangles needs.
pub const FULL_DAMAGE_SHARE: f64 = 0.5;

/// Grows `damage` to cover everything read by a composite whose read region it already touches.
///
/// Runs to a fixpoint, because a grown rectangle can reach a second group's source region; the
/// number of passes is bounded by the number of registered fragments, since each pass can absorb at
/// most one source no earlier pass reached. On exhaustion the whole surface is damaged, which is
/// correct and, at that point, cheaper than continuing.
///
/// After this the damage set is frozen: the emit walk consults it and never adds to it.
///
/// `scale` is the frame's own device scale, and it is the same one the emit walk is given: the two
/// readers of a read extent have to convert the filter chain identically or the region the
/// expansion adds and the region the cull tests are different rectangles.
pub fn expand(
    store: &LayoutStore,
    damage: &mut DamageSet,
    surface: Size<i32, Device>,
    scale: f32,
) -> Expansion {
    let registry = store.read_extents();
    let mut report = Expansion::default();
    if registry.is_empty() || damage.is_full() {
        return report;
    }
    let bound = registry.len();
    let area = f64::from(surface.width.max(0)) * f64::from(surface.height.max(0));
    for pass in 1..=bound {
        report.passes = pass;
        let mut grew = false;
        for frag in registry {
            let Some(extent) = read_extent_of(store, *frag, scale) else {
                continue;
            };
            let source = pixels(extent.source);
            if !damage.intersects(source) || contains(damage, source) {
                continue;
            }
            damage.absorb(source);
            report.absorbed += 1;
            grew = true;
        }
        if !grew {
            return finish(damage, area, report);
        }
    }
    // Every pass grew something, so the fixpoint was not reached inside the bound.
    tracing::warn!(
        registered = registry.len(),
        "damage expansion did not settle; damaging the whole surface"
    );
    damage.set_full();
    report.escalated = true;
    report
}

/// Escalates to a full redraw when the set has grown past what is worth scissoring to.
fn finish(damage: &mut DamageSet, area: f64, mut report: Expansion) -> Expansion {
    if area > 0.0
        && let Some(covered) = damage.area()
        && covered as f64 > area * FULL_DAMAGE_SHARE
    {
        damage.set_full();
        report.escalated = true;
    }
    report
}

/// Whether one rectangle of the set already contains all of `rect`.
///
/// The set's rectangles are pairwise disjoint, so a source region spread across two of them is not
/// contained by either — and that is exactly the case the expansion has to keep absorbing, because
/// the guarantee it buys is that when the pass for a rectangle runs, every pixel that composite
/// reads holds this frame's content.
fn contains(damage: &DamageSet, rect: zgui_geom::Rect<i32, Device>) -> bool {
    damage
        .rects()
        .iter()
        .any(|existing| existing.contains_rect(rect))
}

/// Absorbs the area every subtree removed since the last call occupied, and reports how many roots
/// contributed.
///
/// **Call this while the removed subtrees' geometry still exists** — before the box tree is patched
/// for the change. The document keeps the removed roots until the frame's recycling pass, but the
/// fragments that say *where* they were are replaced when the box tree is rebuilt, and a rectangle
/// read after that is the empty one.
///
/// It takes the roots rather than borrowing them, which is what makes it the consumer: a second
/// reader would find the list already emptied and absorb nothing, silently.
pub fn vacated(document: &mut Document, store: &LayoutStore, damage: &mut DamageSet) -> usize {
    let removed = document.take_removed();
    let mut absorbed = 0;
    for index in removed {
        let node = document.store().key_of(index);
        let ink = zgui_layout::fragment::index::ink_of(store, node);
        if ink.is_empty() {
            continue;
        }
        damage.absorb(pixels(ink));
        absorbed += 1;
    }
    absorbed
}

#[cfg(test)]
mod tests {
    use zgui_bits::DamageSet;
    use zgui_geom::{Device, Point, Rect, Size};

    use super::contains;

    #[test]
    fn a_region_spread_across_two_rectangles_is_contained_by_neither() {
        let mut damage: DamageSet = DamageSet::new();
        damage.absorb(Rect::new(Point::new(0, 0), Size::new(10, 10)));
        damage.absorb(Rect::new(Point::new(100, 0), Size::new(10, 10)));
        let across: Rect<i32, Device> = Rect::new(Point::new(0, 0), Size::new(110, 10));
        assert!(!contains(&damage, across));
        assert!(contains(
            &damage,
            Rect::new(Point::new(2, 2), Size::new(4, 4))
        ));
    }
}
