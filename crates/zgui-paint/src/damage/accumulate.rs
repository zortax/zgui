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
use zgui_geom::{Device, Edges, Rect, Size};
use zgui_layout::fragment::diff::pixels;
use zgui_layout::{BoxKey, FragKey, LayoutStore};
use zgui_scene::Placements;

use crate::damage::ink::{Reads, read_extent_of};

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
    /// The box whose backdrop may filter the copy kept from the last frame, if there is one.
    ///
    /// Set only where the damage was grown the cheap way for it, so it is not a hint: the emitter
    /// marks exactly this box's backdrop, and the renderer is then entitled to read pixels this
    /// frame never wrote.
    pub keeping: Option<BoxKey>,
}

/// What the kept backdrop copy holds, between frames.
///
/// The whole of the state behind [`Expansion::keeping`]. A copy may only be kept when the frame
/// before this one filled it for the same backdrop over the same region of the same surface — so
/// the first frame a frosted panel appears on pays for the whole copy, and every frame after it
/// pays for what moved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BackdropMemory {
    /// The surface the copy is of.
    surface: Size<i32, Device>,
    /// The box whose backdrop it holds what lies beneath, and the region that was read.
    held: Option<(BoxKey, Rect<i32, Device>)>,
    /// Whether the frame that last filled the copy left it worth keeping.
    usable: bool,
}

impl BackdropMemory {
    /// Records what the frame just emitted, which is what decides whether the next may keep the
    /// copy it left behind.
    ///
    /// Two things disqualify it and only the emit walk knows either, because both are about what
    /// *encloses* a backdrop rather than about the backdrop itself. One inside a group filters that
    /// group's own target rather than the composite, and the kept copy is of the composite — not
    /// the same picture, and not even the same format. And a frame that drew more than one leaves
    /// the copy holding whatever the last of them captured, which is not what the first would read.
    ///
    /// Both are answered a frame late on purpose. The alternative is to answer them while the walk
    /// runs, which is after the damage has been grown on the strength of the answer.
    pub fn emitted(&mut self, backdrops: usize, nested: bool) {
        self.usable = backdrops == 1 && !nested;
    }

    /// Whether `candidate` may read the copy kept from the last frame, and remembers it either way.
    ///
    /// The surface is part of it because the copy is allocated against the surface: one of a
    /// different size holds nothing this one can read.
    fn admit(
        &mut self,
        surface: Size<i32, Device>,
        candidate: Option<(BoxKey, Rect<i32, Device>)>,
    ) -> Option<BoxKey> {
        let kept = self.usable && self.surface == surface && self.held == candidate;
        self.surface = surface;
        self.held = candidate;
        kept.then_some(candidate?.0)
    }
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
/// `placements` puts each read extent on the device, where the damage is measured.
pub fn expand(
    store: &LayoutStore,
    damage: &mut DamageSet,
    surface: Size<i32, Device>,
    scale: f32,
    placements: &Placements,
    memory: &mut BackdropMemory,
) -> Expansion {
    let registry = store.read_extents();
    let mut report = Expansion::default();
    if registry.is_empty() || damage.is_full() {
        // Nothing is kept across a frame that redraws everything, and nothing needs to be: a full
        // set covers whatever any filter reads, so the next frame's copy is complete either way.
        memory.held = None;
        return report;
    }
    let area = f64::from(surface.width.max(0)) * f64::from(surface.height.max(0));

    // Exactly one, because there is one kept copy. Two frosted panels would each want it, and the
    // second would overwrite what the first left for the frame after.
    let sole = sole_backdrop(store, registry, scale);
    let keeping = memory.admit(surface, sole.map(|(box_, _, source)| (box_, source)));

    if !settle(
        store,
        registry,
        damage,
        scale,
        placements,
        keeping,
        &mut report,
    ) {
        return give_up(damage, registry.len(), report);
    }
    let Some((box_, bounds, source)) = keeping.and(sole) else {
        return finish(damage, area, report);
    };
    // The chain's reach, read off the two rectangles rather than recomputed from the filters: the
    // one was made by inflating the other, so their difference *is* it.
    let reach = (bounds.left() - source.left())
        .max(bounds.top() - source.top())
        .max(source.right() - bounds.right())
        .max(source.bottom() - bounds.bottom())
        .max(0);

    // What a kept backdrop costs: not everything it reads, but everywhere its answer *changed* —
    // which is whatever moved beneath it, widened by how far the filter carries a pixel. A spinner
    // turning under a full-window scrim is sixteen pixels and a halo rather than the window.
    let mut grew = false;
    for rect in changed_by(damage, bounds, reach) {
        if contains(damage, rect) {
            continue;
        }
        damage.absorb(rect);
        report.absorbed += 1;
        grew = true;
    }
    if !grew {
        report.keeping = Some(box_);
        return finish(damage, area, report);
    }
    // The widening can have reached a *content* filter's source, and that one needs its whole read
    // region painted. Settling again may then widen the damage under the backdrop once more, and
    // chasing the two around costs more than it saves — so the copy is renewed instead, which is
    // what every frame did before there was one to keep.
    let settled = report.absorbed;
    if !settle(
        store,
        registry,
        damage,
        scale,
        placements,
        keeping,
        &mut report,
    ) {
        return give_up(damage, registry.len(), report);
    }
    match report.absorbed > settled {
        true => {
            memory.held = None;
            if !contains(damage, source) {
                damage.absorb(source);
            }
            let _ = settle(
                store,
                registry,
                damage,
                scale,
                placements,
                None,
                &mut report,
            );
        }
        false => report.keeping = Some(box_),
    }
    finish(damage, area, report)
}

/// Grows `damage` over every read extent it touches, to a fixpoint, leaving `keeping` alone.
///
/// Answers whether it settled inside the bound. The bound is the number of registered fragments,
/// since each pass can absorb at most one source no earlier pass reached.
fn settle(
    store: &LayoutStore,
    registry: &[FragKey],
    damage: &mut DamageSet,
    scale: f32,
    placements: &Placements,
    keeping: Option<BoxKey>,
    report: &mut Expansion,
) -> bool {
    for pass in 1..=registry.len() {
        report.passes = report.passes.max(pass);
        let mut grew = false;
        for frag in registry {
            // Every fragment of the kept box, because they are all the one panel.
            if keeping.is_some() && store.fragment(*frag).map(|it| it.box_) == keeping {
                continue;
            }
            let Some(extent) = read_extent_of(store, *frag, scale) else {
                continue;
            };
            let Some(fragment) = store.fragment(*frag) else {
                continue;
            };
            let source = pixels(fragment.on_device(extent.source, placements));
            if !damage.intersects(source) || contains(damage, source) {
                continue;
            }
            // Whole, because the test above asks whether one rectangle covers the source.
            damage.absorb_whole(source);
            report.absorbed += 1;
            grew = true;
        }
        if !grew {
            return true;
        }
    }
    false
}

/// Damages the whole surface, for an expansion that never settled.
fn give_up(damage: &mut DamageSet, registered: usize, mut report: Expansion) -> Expansion {
    tracing::warn!(
        registered,
        "damage expansion did not settle; damaging the whole surface"
    );
    damage.set_full();
    report.escalated = true;
    report.keeping = None;
    report
}

/// The one box whose backdrop reads outside what it writes, what it writes, and what it reads.
///
/// `None` where there is none, and where there is more than one.
///
/// **Boxes, not fragments.** A box can hold several fragments and the registry lists every one of
/// them — a modal scrim was found holding seven — while what reaches the display list is one
/// backdrop per *painted* fragment. Counting registry entries finds a crowd where there is one
/// panel, and then no panel ever keeps anything. Their rectangles need not agree either, so the
/// answer is the union: a rule that has to cover whichever fragment the emitter picks covers all
/// of them.
/// The one backdrop a frame may keep a copy for: the box it belongs to, what it writes, and what
/// it reads.
type SoleBackdrop = (BoxKey, Rect<i32, Device>, Rect<i32, Device>);

fn sole_backdrop(store: &LayoutStore, registry: &[FragKey], scale: f32) -> Option<SoleBackdrop> {
    let mut found: Option<SoleBackdrop> = None;
    for frag in registry {
        let Some(extent) = read_extent_of(store, *frag, scale) else {
            continue;
        };
        if extent.reads != Reads::WhatIsBeneath {
            continue;
        }
        let box_ = store.fragment(*frag)?.box_;
        let (bounds, source) = (pixels(extent.bounds), pixels(extent.source));
        found = match found {
            None => Some((box_, bounds, source)),
            Some((held, held_bounds, held_source)) if held == box_ => {
                Some((held, held_bounds.union(bounds), held_source.union(source)))
            }
            Some(_) => return None,
        };
    }
    found
}

/// Where a filter of `reach` over `bounds` answers differently because `damage` moved.
///
/// A pixel of the answer is a weighted sum of its neighbourhood, so it changes exactly when
/// something inside `reach` of it did. Nothing outside `bounds` is drawn by the filter at all.
fn changed_by(damage: &DamageSet, bounds: Rect<i32, Device>, reach: i32) -> Vec<Rect<i32, Device>> {
    damage
        .rects()
        .iter()
        .filter(|rect| rect.intersects(bounds))
        .filter_map(|rect| rect.outset(Edges::uniform(reach)).intersection(bounds))
        .filter(|rect| !rect.is_empty())
        .collect()
}

/// Escalates to a full redraw when the set has grown past what is worth scissoring to.
fn finish(damage: &mut DamageSet, area: f64, mut report: Expansion) -> Expansion {
    if area > 0.0
        && let Some(covered) = damage.area()
        && covered as f64 > area * FULL_DAMAGE_SHARE
    {
        damage.set_full();
        report.escalated = true;
        // A frame that redraws everything renews the copy by drawing it, so there is nothing left
        // for keeping one to save and the invariant stays simple: a kept copy means a damage set
        // that was grown the cheap way.
        report.keeping = None;
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
///
/// `placements` puts the removed pieces' ink on the device, where the damage is measured.
pub fn vacated(
    document: &mut Document,
    store: &LayoutStore,
    damage: &mut DamageSet,
    placements: &Placements,
) -> usize {
    let removed = document.take_removed();
    let mut absorbed = 0;
    for index in removed {
        let node = document.store().key_of(index);
        let ink = zgui_layout::fragment::index::device_ink_of(store, node, placements);
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
