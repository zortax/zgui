//! The coalescing policy itself.

use zgui_bits::DamageSet;
use zgui_geom::{Device, DevicePx, Rect, Size};

use crate::clip::ClipTable;
use crate::id::{ClipId, DrawOrder};
use crate::pass::overlap::{Intervening, Overlap};
use crate::pass::plan::{PlannedItem, PlannedPass, ScenePassPlan};
use crate::pass::{region, trap};
use crate::vector::VectorItem;

/// One entry of the emission stream the policy sweeps.
///
/// Only two things about a primitive matter here: whether it is vector content that a pass has to
/// rasterise, and, if it is not, what it paints — because that is what a composite inserted after
/// it would cover.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Event {
    /// Vector content, by its index in the scene's items.
    Vector(usize),
    /// Anything else, by what it paints.
    Occluder(Rect<DevicePx, Device>),
    /// A point where the renderer changes which target it is drawing into.
    ///
    /// A group's start and its end are both one of these. They carry no geometry because what
    /// matters about them is not what they cover: a pass is composited by **one** draw, inserted
    /// where its last item falls, and a draw is recorded into whichever target is open at that
    /// point and scissored to that target's region. So a pass whose items straddle a group boundary
    /// has every item on the far side of it discarded by a scissor belonging to something else —
    /// which is a drawing that reached the display list, was rasterised, and is not on the screen.
    Boundary,
}

/// Everything the policy reads.
pub(crate) struct Input<'a> {
    /// The emission stream, in draw order.
    pub(crate) events: &'a [Event],
    /// The scene's vector items.
    pub(crate) vectors: &'a [VectorItem],
    /// What is being redrawn this frame.
    pub(crate) damage: &'a DamageSet,
    /// The surface's extent, which regions are clamped to.
    pub(crate) viewport: Size<i32, Device>,
    /// Which reading of rule 3 to apply.
    pub(crate) overlap: Overlap,
}

/// A pass being accumulated.
struct Open {
    /// Indices into the scene's vector items, in draw order.
    items: Vec<usize>,
    /// Their inks, in the same order.
    inks: Vec<Rect<DevicePx, Device>>,
    /// Every non-vector primitive emitted since this pass's first item.
    intervening: Vec<Intervening>,
    /// The highest draw order among the items admitted, which is where the composite belongs.
    ///
    /// **The highest and not the last.** Draw order is allocated from what a primitive overlaps, so
    /// it does not rise with emission order: two panels side by side each start again from just
    /// above the page beneath them, and a drawing in the second can therefore take a *lower* order
    /// than one in the first. One composite carries every item of the pass, so putting it anywhere
    /// below the highest of them buries the rest — a drawing nested inside a card would be
    /// composited under the card's own background and the card would erase it, which is a drawing
    /// that was read, fitted, emitted and rasterised and is not on the screen.
    composite_order: DrawOrder,
    /// Whether the pass refuses further items.
    ///
    /// Set by rule 0: a pass opened for an item whose clip the vector scene cannot express is bound
    /// to *that item's* clip, so admitting a second item would either clip the newcomer by a
    /// stranger's chain or drop the clip the pass exists for.
    sealed: bool,
}

/// Plans a frame's vector passes into `out`.
pub(crate) fn plan(input: Input<'_>, clips: &mut ClipTable, out: &mut ScenePassPlan) {
    out.clear();
    let mut open: Option<Open> = None;

    for event in input.events {
        match *event {
            // Rule 4: a pass ends where the target does. Nothing is carried across, not even the
            // intervening occluders, because the pass that follows begins in a different target.
            Event::Boundary => {
                if let Some(finished) = open.take() {
                    close(finished, clips, &input, out);
                }
            }
            Event::Occluder(bounds) => {
                if let Some(current) = open.as_mut() {
                    current.intervening.push(Intervening {
                        bounds,
                        accumulated: current.inks.len(),
                    });
                }
            }
            Event::Vector(index) => {
                let item = &input.vectors[index];
                // Rule 1, and the only damage cull there is.
                if !is_damaged(item.ink, input.damage) {
                    out.culled += 1;
                    continue;
                }
                // Rule 0.
                let inexpressible = !clips.is_expressible_in_vector_scene(item.clip);
                let starts_fresh = match open.as_ref() {
                    None => true,
                    Some(_) if inexpressible => true,
                    Some(current) if current.sealed => true,
                    // Rule 3.
                    Some(current) => {
                        input
                            .overlap
                            .splits(&current.inks, item.ink, &current.intervening)
                    }
                };
                if starts_fresh && let Some(finished) = open.take() {
                    close(finished, clips, &input, out);
                }
                let current = open.get_or_insert_with(|| Open {
                    items: Vec::new(),
                    inks: Vec::new(),
                    intervening: Vec::new(),
                    composite_order: item.order,
                    sealed: false,
                });
                current.items.push(index);
                current.inks.push(item.ink);
                current.composite_order = current.composite_order.max(item.order);
                current.sealed |= inexpressible;
            }
        }
    }

    if let Some(finished) = open.take() {
        close(finished, clips, &input, out);
    }
}

/// Finishes an accumulating pass.
///
/// Rule 5: one composite carries the whole pass and sits above every item in it, which is only
/// available when nothing painted over a lower item needs to come between. When something does, the
/// items are recorded one pass each — each composite then sits at its own item's order, which is
/// always sound — rather than one of them being drawn in the wrong place.
fn close(open: Open, clips: &mut ClipTable, input: &Input<'_>, out: &mut ScenePassPlan) {
    let orders: Vec<_> = open
        .items
        .iter()
        .map(|index| input.vectors[*index].order)
        .collect();
    if trap::traps(&open.inks, &orders, &open.intervening, open.composite_order) {
        for index in &open.items {
            record(core::slice::from_ref(index), clips, input, out);
        }
        return;
    }
    record(&open.items, clips, input, out);
}

/// Records one pass covering exactly `items`: resolves its shared clip, splits out the residuals,
/// decides whether it can be composited per item, and records the region.
///
/// The composite goes at the highest order among `items`, the only order that is above all of them.
fn record(items: &[usize], clips: &mut ClipTable, input: &Input<'_>, out: &mut ScenePassPlan) {
    let inks: Vec<_> = items
        .iter()
        .map(|index| input.vectors[*index].ink)
        .collect();
    let Some(bounds) = inks.iter().copied().reduce(Rect::union) else {
        return;
    };
    let region = region::aligned(bounds, input.viewport);
    if region.is_empty() {
        return;
    }

    // Rule 2's other half: the pass's clip is the deepest chain every item of it applies.
    let pass_clip = items
        .iter()
        .map(|index| input.vectors[*index].clip)
        .reduce(|left, right| clips.common_ancestor(left, right))
        .unwrap_or(ClipId::ROOT);

    let first = out.items.len();
    // The whole-pixel inks, kept because they and not the float ones are what a per-item composite
    // covers, so they and not the float ones are what decides whether one is sound.
    let mut covered = Vec::with_capacity(items.len());
    for index in items {
        let item = &input.vectors[*index];
        let residual = clips.residual(item.clip, pass_clip);
        out.clip_layers += clips.depth(residual) as usize;
        let ink = region::covering(item.ink);
        covered.push(ink);
        out.items.push(PlannedItem {
            item: *index,
            residual,
            clip: item.clip,
            ink: Rect::new(
                zgui_geom::Point::new(
                    ink.origin.x - region.origin.x,
                    ink.origin.y - region.origin.y,
                ),
                ink.size,
            ),
        });
    }

    let composite_order = items
        .iter()
        .map(|index| input.vectors[*index].order)
        .max()
        .unwrap_or_default();
    out.passes.push(PlannedPass {
        items: first..out.items.len(),
        region,
        clip: pass_clip,
        instanced: pairwise_disjoint(&covered),
        composite_order,
    });
}

/// Whether `ink` meets anything being redrawn.
fn is_damaged(ink: Rect<DevicePx, Device>, damage: &DamageSet) -> bool {
    damage.intersects(region::covering(ink))
}

/// Whether no two of these rectangles overlap.
///
/// Quadratic in the size of one pass, which is bounded by the same sweep rule 3 already performs,
/// and is what decides whether the pass can be composited one item at a time.
///
/// It is asked about **whole-pixel** rectangles, and that is the whole of why it is sound. A backend
/// compositing a pass one item at a time draws one quad per item, and a quad covers whole pixels: two
/// items whose ink is disjoint only in fractions of a pixel — one ending at 40.3, the next starting
/// at 40.5 — share the pixel column between them, and that column would be read out of the scratch
/// and blended twice. Rounding first is what makes "no two overlap" mean "no texel is composited
/// twice".
fn pairwise_disjoint(inks: &[Rect<i32, Device>]) -> bool {
    inks.iter().enumerate().all(|(index, left)| {
        inks[index + 1..]
            .iter()
            .all(|right| !left.intersects(*right))
    })
}
