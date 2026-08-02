//! Recomputing fragments for a subtree, and recording exactly what changed.
//!
//! This is the one walk that writes the fragment tree, and it does five things at once because all
//! five need the same descent and the same unwind:
//!
//! * it composes each box's absolute position, rounds it to the device grid and adds the scroll and
//!   sticky offsets in;
//! * it compares the result against the fragment that was there before and records whether the
//!   piece merely moved or has to be painted again;
//! * it absorbs what changed into the frame's damage;
//! * it folds, on the unwind, the union of the subtree's ink, whether anything below blends, and
//!   whether the subtree's pieces overlap each other;
//! * and it keeps the hit index and the read-extent registry in step, one entry at a time.
//!
//! # Why the folds are on the unwind and not on an ancestor walk
//!
//! The walk descends only what changed. A fold performed on the unwind reads each child's *cached*
//! answer, so it is correct for a subtree the walk never entered; a walk upwards from each blending
//! or filtered fragment would never run at all for a fragment that was not visited. The two look
//! equivalent and are not, and the difference is only visible in the frame where something under an
//! untouched blurred panel animates.

use zgui_bits::{DamageSet, Dirty};
use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Rect, Size};
use zgui_profile::{Counter, counter};

use crate::fragment::build::{Descent, Placed, Tables};
use crate::fragment::hit::HitIndex;
use crate::fragment::{FragKey, FragmentFlags, FragmentKind, build};
use crate::tree::store::LayoutStore;

mod damage;
mod dirty;
mod geometry;
mod rigid;
pub mod split;

pub use crate::fragment::diff::damage::pixels;
pub use crate::fragment::diff::dirty::{DocumentMarks, Everything, FrameDirty, Owed};

use crate::fragment::diff::damage::{absorb, overlaps, pairwise_disjoint};
use crate::fragment::diff::geometry::{Geometry, compare};

/// What a fragment's geometry did between two frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// Nothing about it moved or changed. Its recorded painting can be replayed as it stands.
    Identical,
    /// It is the same size and shape somewhere else, so its recorded painting can be replayed at an
    /// offset rather than produced again.
    TranslatedOnly,
    /// Something other than its position changed, so it has to be painted again.
    Changed,
}

/// The bits that make this pass visit a box at all.
///
/// A box carrying none of them, whose composed geometry is unchanged, contributes nothing this
/// frame: its fragments are already right, its ink is already in its ancestors' unions, and its hit
/// entries already say where it is.
pub const ENTERS: Dirty = Dirty::RELAYOUT
    .union(Dirty::REPOSITION)
    .union(Dirty::REFRAGMENT)
    .union(Dirty::RESTACK)
    .union(Dirty::SCROLL)
    .union(Dirty::REPAINT)
    .union(Dirty::REHIT)
    .union(Dirty::RESHAPE);

/// The bits that mean a box has to be painted again even though nothing about it moved.
///
/// Geometry is compared, so a piece that changed shape is found by comparison alone. Neither of
/// these changes any geometry: a colour is the same rectangle in a different paint, and re-shaped
/// text is the same line box holding different glyphs. A pass that only absorbed what moved would
/// leave both on screen as they were.
const REPAINTS_IN_PLACE: Dirty = Dirty::REPAINT.union(Dirty::RESHAPE);

/// Recomputes fragments for a relaid-out subtree and records what changed.
///
/// The index is threaded in rather than reached for, because it is the same walk that writes it: an
/// index rebuilt afterwards from the finished tree would cost the whole document to service what
/// one row's animation moved.
pub fn rebuild(
    store: &mut LayoutStore,
    hit: &mut HitIndex,
    tables: &mut Tables<'_>,
    dirty: &mut impl FrameDirty,
    root: BoxKey,
    damage: &mut DamageSet,
) {
    // The frame boundary for the coordinate systems. A box that gave one back during the *previous*
    // frame kept it readable for the rest of that frame — the emit walk, the hit index and anything
    // asking where a box was all still resolved it — and this is where it stops. It is here rather
    // than beside the side tables' own frame boundary because the side tables begin their frame at
    // emit time, which is after this walk has already released and re-established.
    tables.spatial.recycle();
    let seed = crate::round::snap::seed(store, root);
    let descent = Descent {
        layout: seed,
        rounded: (seed.0.round(), seed.1.round()),
        ..Descent::root(viewport_of(store, root), tables.spatial.viewport())
    };
    let scale = tables.device.scale;
    let mut pass = Pass {
        store,
        hit,
        tables,
        dirty,
        damage,
        restacked: false,
        // Read once, here, rather than once per moved subtree: how a frame is being measured is
        // decided before the frame and must not change part-way through one.
        passes: split::current(),
    };
    pass.visit(root, descent, None, None);
    // Every rigid move the walk made wrote its entries and left the hierarchy above them for here,
    // so that a scroll repairs it once instead of once per entry. Nothing may query the index
    // between the walk and this line.
    if pass.passes == split::Passes::Together {
        pass.hit.settle();
    } else {
        split::timed(split::Part::Settle, || pass.hit.settle());
    }
    // Retired after the walk rather than during it, because the walk reads a node's marks twice —
    // once to decide whether its subtree settled, and again to decide what a fragment that did not
    // move nonetheless owes — and a phase cleared between the two reads would lose the second.
    //
    // This also clears what the walk itself marked, and that is the point rather than a casualty:
    // those marks record what this pass has just finished doing to a fragment, so carrying them
    // into the next frame would repaint an unchanged fragment once more for every time it changed.
    let root_source = pass.store.node(root).source;
    pass.dirty.retire(root_source, ENTERS);
    let restacked = pass.restacked;
    // Every fragment that ceased to exist since the last pass, including the ones belonging to
    // boxes this walk never reached because they are no longer in the tree. Their entries would
    // otherwise answer hits for ever, at the place the deleted content used to be, in front of
    // whatever now occupies it.
    for frag in store.drain_retired() {
        hit.remove(frag);
    }
    // And every coordinate system named after a box that is no longer there. Given back after the
    // walk rather than during it, because a box the walk did visit has just re-established its own
    // and must not have it taken away again; and given back rather than left, because a name that is
    // never returned is a slot in the dense buffer that is never reused.
    for gone in store.drain_retired_boxes() {
        tables.spatial.release(zgui_scene::PropertyOwner::of(gone));
    }
    // Painting order is a property of the whole document, so a fragment that did not exist before
    // has no place in it and no incremental update can invent one. That, and an index that has been
    // moved entry by entry until its hierarchy is no longer a good one, are the two cases the bulk
    // build exists for — and they are the only two, because a transform transition marks a subtree
    // on every tick and must not cost the document.
    if restacked || hit.should_rebuild() {
        hit.rebuild(store, scale);
    }
    // Published after every retirement this pass owed has been taken, so the figure is what the
    // next frame inherits rather than what this one built before it gave anything back.
    counter::set(Counter::FragmentsLive, u64::from(store.fragment_count()));
    crate::invariants::check_if_enabled(store, hit);
}

/// The viewport a subtree is composed inside, taken from the document's own root box.
fn viewport_of(store: &LayoutStore, fallback: BoxKey) -> Size<DevicePx, Device> {
    let key = store.root().unwrap_or(fallback);
    store
        .layout_of(key)
        .map(|layout| layout.size)
        .unwrap_or(Size::new(DevicePx(0.0), DevicePx(0.0)))
}

/// What one box's subtree reported to the box above it.
#[derive(Clone, Copy, Debug)]
struct Folded {
    /// The union of this box's ink and everything below it.
    subtree_ink: Rect<DevicePx, Device>,
    /// Whether anything at or below this box blends with what is behind it.
    blending: bool,
    /// Whether nothing in this subtree overlaps anything else in it.
    disjoint: bool,
    /// Whether every piece in this subtree moves by the same vector as the box above it.
    rigid: bool,
}

/// The state one walk carries.
struct Pass<'a, 'b, D: FrameDirty> {
    /// The boxes, their results and their fragments.
    store: &'a mut LayoutStore,
    /// The index this walk keeps in step.
    hit: &'a mut HitIndex,
    /// Where clips and transforms are interned, and what the device is.
    tables: &'a mut Tables<'b>,
    /// What is dirty, and where to record what this pass makes dirty.
    dirty: &'a mut D,
    /// What must be redrawn.
    damage: &'a mut DamageSet,
    /// Whether this walk produced a fragment that has no place in the painting order yet.
    restacked: bool,
    /// Whether a subtree that only moved is offset in one descent or in one descent per duty.
    passes: split::Passes,
}

impl<D: FrameDirty> Pass<'_, '_, D> {
    /// Composes one box, its fragments and everything below it.
    ///
    /// `generator` is the element the box directly above this one was styled from, which is what an
    /// anonymous box's own invalidation is read out of — see [`Pass::generator_of`].
    fn visit(
        &mut self,
        key: BoxKey,
        from: Descent,
        parent: Option<FragKey>,
        generator: Option<zgui_dom::NodeKey>,
    ) -> Folded {
        counter::bump(Counter::NodesVisited);
        let Some(placed) = build::place(self.store, self.tables, key, from) else {
            return Folded {
                subtree_ink: Rect::ZERO,
                blending: false,
                disjoint: true,
                rigid: true,
            };
        };
        let state = self.store.state_mut(key);
        state.snapped = placed.snapped;
        state.composed = state.unrounded;
        // Read before it is replaced: the difference between the two is how far this box's contents
        // have moved since they were last composed, which is the whole input to the offsetting path.
        let previous_shift = state.composed_shift;
        state.composed_shift = placed.descent.shift;
        let movement = self.movement(&placed, previous_shift);

        let owed = self.owed_by(key, generator);
        let own = owed.own;
        let (fragments, moved) = self.write_fragments(key, &placed, parent, &owed);

        // A box's first fragment is its own piece and every later one hangs below it, so the later
        // ones are pieces of the subtree and not part of the box's own ink. Folding them into it
        // would hide the commonest overlap there is — a background and the lines of text drawn on
        // top of it — behind a union that can never overlap itself.
        let mut ink = Rect::ZERO;
        let mut child_inks: Vec<Rect<DevicePx, Device>> = Vec::new();
        for (slot, &frag) in fragments.iter().enumerate() {
            let Some(fragment) = self.store.fragment(frag) else {
                continue;
            };
            if slot == 0 {
                ink = fragment.ink;
            } else {
                child_inks.push(fragment.ink);
            }
        }

        let children = self.store.node(key).children.clone();
        let mut blending = placed.blends;
        let mut disjoint = true;
        let mut rigid = placed.rigid;
        let mut subtree_ink = child_inks
            .iter()
            .fold(ink, |held, piece| held.union(*piece));
        // A clean child may only be left alone if this box did not move: its own position is
        // composed from this one's, so a parent that shifted by a pixel invalidates every
        // descendant's absolute geometry however clean the descendant itself is. The box's own
        // marks count too, because some of them move a child without moving the box — a scroll
        // offset changes what descendants are composed against and leaves the container where it
        // was. What must *not* be consulted here is what the subtree owes: that is the union over
        // every descendant, so testing it would dismiss the skip for every sibling of anything
        // dirty, and the per-child test below is the refinement of exactly that question.
        let settled = !moved && !own.intersects(ENTERS);
        for child in children {
            let clean = self.can_skip(child, owed.node);
            let folded = match movement {
                _ if settled && clean => self.cached(child),
                Some(movement) if clean && self.can_translate(child) => {
                    self.translate(child, movement)
                }
                _ => self.visit(child, placed.descent, fragments.first().copied(), owed.node),
            };
            child_inks.push(folded.subtree_ink);
            blending |= folded.blending;
            disjoint &= folded.disjoint;
            rigid &= folded.rigid;
            subtree_ink = subtree_ink.union(folded.subtree_ink);
        }

        // `own ink disjoint from everything below it, everything below it pairwise disjoint, and
        // every child disjoint in itself` — where "everything below it" is this box's own later
        // pieces as well as its children's subtrees, because in the fragment tree those pieces are
        // children of the box's own fragment. Answered over ink, which is layout geometry, so no
        // damage set and no emission decision is an input to it: a frame that painted half the
        // subtree decides exactly what a frame that painted all of it decides.
        disjoint &= !child_inks.iter().any(|child| overlaps(ink, *child));
        disjoint &= pairwise_disjoint(&child_inks);

        self.record_fold(&fragments, subtree_ink, blending, disjoint, rigid);
        Folded {
            subtree_ink,
            blending,
            disjoint,
            rigid,
        }
    }

    /// How far this box's contents have moved, when that is all that happened to them.
    ///
    /// `None` unless the offset is the *only* difference, which takes three things. The origin the
    /// contents are snapped against has to be the one they were snapped against before, because
    /// snapping rounds cumulative absolute edges and a different origin rounds some of them the
    /// other way. Nothing above may carry a transform, because then a vector in this box's space is
    /// not a vector on the device. And the offset has to be non-zero, since a subtree that did not
    /// move is one the caller leaves alone entirely rather than offsetting by nothing.
    fn movement(&self, placed: &Placed, previous: (f32, f32)) -> Option<rigid::Move> {
        let by = (
            placed.descent.shift.0 - previous.0,
            placed.descent.shift.1 - previous.1,
        );
        let usable =
            placed.descent.layout_stable && placed.descent.matrix.is_none() && by != (0.0, 0.0);
        usable.then_some(rigid::Move {
            by,
            clip: placed.descent.clip,
            shift: placed.descent.shift,
        })
    }

    /// Whether a clean child's cached fold can stand in for visiting it.
    ///
    /// Being clean is not enough on its own, and the difference is a wrong frame rather than a slow
    /// one. A box is marked when *its own* content changes; it is not marked when a sibling's
    /// does — and a sibling that grew moves everything the flow places after it. A caret drawn
    /// beside a field, a label beside a count, a row of buttons after a growing label: none of them
    /// is dirty, all of them move, and a skip that consulted only the marks would leave every one
    /// of them composed where it stood last frame while the thing beside it grew through it.
    ///
    /// So the layout result is consulted as well. The engine's answer for this box is compared with
    /// the answer its standing fragments were composed from, which is the whole remaining input to
    /// their geometry once the caller has established that this box's parent did not move.
    fn can_skip(&self, child: BoxKey, generator: Option<zgui_dom::NodeKey>) -> bool {
        let owed = self.owed_by(child, generator);
        !owed.own.intersects(ENTERS)
            && !owed.subtree.intersects(ENTERS)
            && !self.store.fragments_of_box(child).is_empty()
            && self
                .store
                .state(child)
                .is_some_and(|state| state.unrounded == state.composed)
    }

    /// What one box owes, read under the element it came from or the one it was generated for.
    ///
    /// [`Owed::of`] states the rule and why an anonymous box gets its generator's marks rather than
    /// everything; this supplies the two names it needs. `generator` is the element the box above
    /// this one was read under, which the walk carries down.
    fn owed_by(&self, key: BoxKey, generator: Option<zgui_dom::NodeKey>) -> Owed {
        let source = self.store.get(key).and_then(|node| node.source);
        Owed::of(self.dirty, source, generator)
    }

    /// What a subtree the walk did not descend into reported last time.
    fn cached(&self, child: BoxKey) -> Folded {
        let fragments = self.store.fragments_of_box(child);
        let mut subtree_ink = Rect::ZERO;
        let mut blending = false;
        let mut disjoint = true;
        let mut rigid = true;
        for (index, frag) in fragments.iter().enumerate() {
            let Some(fragment) = self.store.fragment(*frag) else {
                continue;
            };
            subtree_ink = if index == 0 {
                fragment.subtree_ink
            } else {
                subtree_ink.union(fragment.subtree_ink)
            };
            blending |= fragment
                .flags
                .contains(FragmentFlags::HAS_BLENDING_DESCENDANT);
            disjoint &= fragment.subtree_disjoint;
            rigid &= fragment.subtree_rigid;
        }
        Folded {
            subtree_ink,
            blending,
            disjoint,
            rigid,
        }
    }

    /// Writes this box's fragments, reusing the names of the ones that are still there.
    fn write_fragments(
        &mut self,
        key: BoxKey,
        placed: &Placed,
        parent: Option<FragKey>,
        owed: &Owed,
    ) -> (Vec<FragKey>, bool) {
        let kinds = self.kinds_of(key);
        // Names are kept for as long as the box keeps drawing the same things in the same
        // positions, because a name is what the hit index, the recorded painting and the previous
        // frame's damage all refer to. The first position that draws something different is where
        // reuse stops.
        let keep = kinds
            .iter()
            .enumerate()
            .take_while(|(slot, kind)| self.store.reusable_fragment(key, *slot, **kind).is_some())
            .count();
        self.retire(key, keep);

        let mut written = Vec::with_capacity(kinds.len());
        let mut moved = false;
        for (slot, kind) in kinds.into_iter().enumerate() {
            let geometry = self.geometry_for(key, placed, slot, kind);
            let parent = if slot == 0 {
                parent
            } else {
                written.first().copied()
            };
            let frag = match self.store.reusable_fragment(key, slot, kind) {
                Some(frag) => frag,
                None => {
                    self.restacked = true;
                    self.store.insert_fragment(key, |fragment| {
                        fragment.kind = kind;
                    })
                }
            };
            moved |= self.update(frag, kind, &geometry, parent, owed) != Change::Identical;
            written.push(frag);
        }
        (written, moved)
    }

    /// Drops every fragment of a box beyond the first `keep`, damaging what they covered.
    ///
    /// The rectangle a destroyed fragment occupied is nobody's ink from here on, so it is absorbed
    /// now or it is never absorbed at all and last frame's pixels stay on the screen.
    fn retire(&mut self, key: BoxKey, keep: usize) {
        let stale: Vec<FragKey> = self
            .store
            .fragments_of_box(key)
            .iter()
            .skip(keep)
            .copied()
            .collect();
        for frag in stale {
            if let Some(fragment) = self.store.fragment(frag) {
                let gone = fragment.subtree_ink;
                absorb(self.damage, gone);
            }
        }
        // The index is not touched here: `truncate_fragments` records what it destroyed, and the
        // one drain after the walk unregisters every destroyed name by the same route whether its
        // box was visited or deleted.
        self.store.truncate_fragments(key, keep);
    }

    /// What each fragment of this box draws, in the order it draws them.
    fn kinds_of(&self, key: BoxKey) -> Vec<FragmentKind> {
        let node = self.store.node(key);
        // A drawing before replaced content: an element carrying outlines is drawing them itself,
        // and nothing outside the document has been asked for a picture of it.
        let own = match (node.draws_vector, node.replaced) {
            (true, _) => FragmentKind::Vector,
            (false, Some(content)) => FragmentKind::Replaced { content },
            (false, None) => FragmentKind::Box,
        };
        let mut kinds = vec![own];
        if let Some(resolution) = self.store.inline_resolution(key) {
            for index in 0..resolution.lines.len() {
                kinds.push(FragmentKind::Line {
                    paragraph: resolution.paragraph,
                    line: u16::try_from(index).unwrap_or(u16::MAX),
                });
            }
        }
        // After the lines, so that editing the text inside a scrollport does not renumber the
        // slots its bars occupy — a name is what the hit index, the recorded painting and last
        // frame's damage all refer to, and reuse stops at the first slot that draws something else.
        if let Some(layout) = self.store.layout_of(key) {
            kinds.extend(crate::scroll_region::bar::kinds(&layout));
        }
        kinds
    }

    /// One fragment's geometry: the box's own for its first fragment, and one line's for the rest.
    fn geometry_for(
        &self,
        key: BoxKey,
        placed: &Placed,
        slot: usize,
        kind: FragmentKind,
    ) -> Geometry {
        if slot == 0 {
            return Geometry {
                border_box: placed.border_box,
                padding_box: placed.padding_box,
                content_box: placed.content_box,
                border: placed.border,
                padding: placed.padding,
                ink: placed.ink,
                local_ink: placed.local_ink,
                flags: placed.flags,
                clip: placed.clip,
                clip_transform: placed.clip_transform,
                transform: placed.transform,
                transform_hash: placed.transform_hash,
                stacking: placed.stacking,
                scroll: placed.scroll,
                reads_outside: placed.reads_outside,
            };
        }
        // A bar sits in the gutter, which is inside the box's padding box and outside its content
        // box, so it is placed from the box's own rectangles rather than from anything the content
        // produced — and it is drawn under the chain that clips the *scroller*, never under the one
        // the scroller imposes on what it scrolls, or it would disappear along with the content.
        if let FragmentKind::Scrollbar { axis, part } = kind {
            let rect = crate::scroll_region::bar::rect(&self.scrollport(key, placed), axis, part);
            return Geometry {
                border_box: rect,
                padding_box: rect,
                content_box: rect,
                border: zgui_geom::Edges::ZERO,
                padding: zgui_geom::Edges::ZERO,
                ink: rect,
                local_ink: rect,
                flags: FragmentFlags::EMPTY,
                clip: placed.clip,
                clip_transform: placed.clip_transform,
                transform: placed.transform,
                transform_hash: placed.transform_hash,
                stacking: placed.stacking,
                scroll: placed.scroll,
                reads_outside: false,
            };
        }
        let line = match kind {
            FragmentKind::Line { line, .. } => line as usize,
            _ => 0,
        };
        let rect = self
            .store
            .inline_resolution(key)
            .and_then(|resolution| resolution.lines.get(line))
            .map(|line| build::line_rect(placed.content_box, line))
            .unwrap_or(Rect::ZERO);
        Geometry {
            border_box: rect,
            padding_box: rect,
            content_box: rect,
            border: zgui_geom::Edges::ZERO,
            padding: zgui_geom::Edges::ZERO,
            ink: rect,
            local_ink: rect,
            // A line is drawn inside its own box's clip and transform, and establishes neither.
            flags: FragmentFlags::EMPTY,
            clip: placed.descent.clip,
            // A line is drawn under the clip its own box imposes, which that box measured in its
            // own space — the space its own transform maps onto the device.
            clip_transform: placed.transform,
            transform: placed.transform,
            transform_hash: placed.transform_hash,
            stacking: placed.stacking,
            scroll: placed.scroll,
            reads_outside: false,
        }
    }

    /// The scrollport one box's bars are placed in.
    ///
    /// The offset is read here rather than carried on [`Placed`] because it is the one input to a
    /// bar that is not layout: the rest is this frame's composed geometry, and the offset is state
    /// the fragment pass is handed.
    fn scrollport(&self, key: BoxKey, placed: &Placed) -> crate::scroll_region::bar::Scrollport {
        let content = self
            .store
            .layout_of(key)
            .map(|layout| layout.content_size)
            .unwrap_or(Size::new(DevicePx(0.0), DevicePx(0.0)));
        let offset = self.store.get(key).and_then(|node| node.source).map_or(
            zgui_geom::Point::new(DevicePx(0.0), DevicePx(0.0)),
            |element| self.tables.scroll.of(element),
        );
        crate::scroll_region::bar::Scrollport {
            inner: placed.padding_box.inset(placed.padding),
            content_box: placed.content_box,
            content,
            offset,
        }
    }

    /// Compares one fragment against what was there, writes the new geometry and records the
    /// consequences.
    fn update(
        &mut self,
        frag: FragKey,
        kind: FragmentKind,
        next: &Geometry,
        parent: Option<FragKey>,
        owed: &Owed,
    ) -> Change {
        counter::bump(Counter::FragmentsDiffed);
        // The fragment's own element, which an anonymous box has none of, decides only whether this
        // piece carries accessibility semantics. What it *owes* is `owed`, which for an anonymous
        // box is read under the element it was generated for — see [`Owed::of`].
        let node = match self.store.fragment(frag) {
            Some(fragment) => fragment.node,
            None => return Change::Identical,
        };
        let previous = self.store.fragment(frag).cloned();
        // A name is kept across a change of content — a line that is still line two of its box is
        // still the same line — so the content is compared here rather than at the name. Nothing
        // else would catch it: the geometry of a line whose characters changed for characters of
        // the same width is identical, and the recorded painting of the old ones would be replayed
        // straight back onto the screen.
        let change = match previous.as_ref() {
            Some(previous) if previous.kind != kind => Change::Changed,
            Some(previous) => compare(previous, next),
            None => Change::Changed,
        };

        if let Some(fragment) = self.store.fragment_mut(frag) {
            fragment.kind = kind;
            fragment.parent = parent;
            let kept = fragment
                .flags
                .contains(FragmentFlags::HAS_BLENDING_DESCENDANT);
            fragment.border_box = next.border_box;
            fragment.padding_box = next.padding_box;
            fragment.content_box = next.content_box;
            fragment.border = next.border;
            fragment.padding = next.padding;
            fragment.ink = next.ink;
            fragment.local_ink = next.local_ink;
            fragment.clip = next.clip;
            fragment.clip_transform = next.clip_transform;
            fragment.transform = next.transform;
            fragment.transform_hash = next.transform_hash;
            fragment.stacking = next.stacking;
            fragment.scroll = next.scroll;
            fragment.flags = if kept {
                next.flags.union(FragmentFlags::HAS_BLENDING_DESCENDANT)
            } else {
                next.flags
            };
            if change != Change::Identical {
                counter::bump(Counter::FragmentsRebuilt);
            }
        }
        self.store.set_read_extent(frag, next.reads_outside);

        let own = owed.own;
        match change {
            Change::Identical => {
                // A colour change moves nothing, and neither does re-shaped text of the same
                // extent, so the comparison above cannot see either. Without this absorb the two
                // commonest frames there are — a hover repainting one button, and a counter whose
                // digit changed width for width — put nothing in the damage set at all and paint
                // nothing.
                if own.intersects(REPAINTS_IN_PLACE) {
                    absorb(self.damage, next.ink);
                }
                if own.contains(Dirty::REHIT) {
                    self.touch_hit(frag, change);
                }
            }
            Change::TranslatedOnly => {
                self.dirty.mark(
                    owed.node,
                    Dirty::REPOSITION | Dirty::REHIT | self.a11y(node),
                );
                if let Some(previous) = &previous {
                    absorb(self.damage, previous.ink);
                }
                absorb(self.damage, next.ink);
                self.touch_hit(frag, change);
            }
            Change::Changed => {
                self.dirty
                    .mark(owed.node, Dirty::REPAINT | Dirty::REHIT | self.a11y(node));
                if let Some(previous) = &previous {
                    absorb(self.damage, previous.ink);
                }
                absorb(self.damage, next.ink);
                self.touch_hit(frag, change);
            }
        }
        change
    }

    /// The accessibility bit a moved fragment owes, which is none unless its node means something.
    ///
    /// An accessibility node's bounds are geometry, so a control that moved is a control whose
    /// node changed — and a screen reader whose highlight is a frame behind is one whose user is
    /// pointed at the wrong thing. The declaration test is what keeps this proportional: a
    /// scrolled list moves every box in it and only the ones that declared what they are owe an
    /// accessibility node.
    fn a11y(&self, node: Option<zgui_dom::NodeKey>) -> Dirty {
        if self.dirty.is_semantic(node) {
            Dirty::A11Y
        } else {
            Dirty::empty()
        }
    }

    /// Puts one fragment's entry back in the index, keeping the painting order it already had.
    ///
    /// Keeping it is the point: moving a fragment does not change where it sits in painting order,
    /// and re-deriving that order would mean walking the document. A fragment with no entry yet has
    /// no place in the order at all, which is what the bulk build after the walk is for.
    ///
    /// A fragment that only moved goes in through the index's translation path, which does not
    /// count towards the churn that triggers a bulk rebuild — see
    /// [`HitIndex::translate`](crate::fragment::hit::HitIndex::translate).
    fn touch_hit(&mut self, frag: FragKey, change: Change) {
        let scale = self.tables.device.scale;
        let held = self.hit.entry(frag).map(|entry| entry.order).unwrap_or(0);
        if let Some(entry) = crate::fragment::hit::entry_for(self.store, frag, held, scale) {
            match change {
                Change::TranslatedOnly => self.hit.translate(frag, entry),
                Change::Identical | Change::Changed => self.hit.update(frag, entry),
            }
        }
    }

    /// Writes the three unwind answers onto this box's fragments.
    fn record_fold(
        &mut self,
        fragments: &[FragKey],
        subtree_ink: Rect<DevicePx, Device>,
        blending: bool,
        disjoint: bool,
        rigid: bool,
    ) {
        for &frag in fragments {
            let Some(fragment) = self.store.fragment_mut(frag) else {
                continue;
            };
            fragment.subtree_ink = subtree_ink;
            fragment.subtree_disjoint = disjoint;
            fragment.subtree_rigid = rigid;
            fragment.flags = if blending {
                fragment.flags.union(FragmentFlags::HAS_BLENDING_DESCENDANT)
            } else {
                fragment
                    .flags
                    .without(FragmentFlags::HAS_BLENDING_DESCENDANT)
            };
        }
    }
}
