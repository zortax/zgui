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
use zgui_scene::ClipId;

use crate::fragment::build::{Descent, Placed, Tables};
use crate::fragment::hit::HitIndex;
use crate::fragment::{FragKey, FragmentFlags, FragmentKind, build};
use crate::tree::store::LayoutStore;

mod damage;
mod dirty;
mod geometry;
mod rigid;
mod scratch;
pub mod split;

pub use crate::fragment::diff::damage::pixels;
pub use crate::fragment::diff::dirty::{DocumentMarks, Everything, FrameDirty, Owed};
pub use crate::fragment::diff::scratch::DiffScratch;

use crate::fragment::diff::damage::{absorb, overlaps, pairwise_disjoint};
use crate::fragment::diff::geometry::{Geometry, compare, repositioned_within};

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
) -> RigidMoves {
    let mut scratch = DiffScratch::default();
    rebuild_in(&mut scratch, store, hit, tables, dirty, root, damage)
}

/// The same walk in a caller's reusable buffers, which is what a per-frame caller wants.
#[expect(clippy::too_many_arguments, reason = "the wrapped signature plus one")]
pub fn rebuild_in(
    scratch: &mut DiffScratch,
    store: &mut LayoutStore,
    hit: &mut HitIndex,
    tables: &mut Tables<'_>,
    dirty: &mut impl FrameDirty,
    root: BoxKey,
    damage: &mut DamageSet,
) -> RigidMoves {
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
    // Deliberately *not* seeded with what the set already holds. A frame runs this walk more than
    // once — a scroll is delivered to the document between two of them — and the damage standing at
    // entry is the earlier pass's, which for a scrolling document is mostly that pass's own rigid
    // movement. Folding it in here would label a movement as something other than a movement on
    // every frame that reports a scroll, which is every frame of a glide. What the frame damaged
    // before *any* pass ran is the caller's to remember, because only the caller knows which pass
    // was the first.
    let mut pass = Pass {
        store,
        hit,
        tables,
        dirty,
        damage,
        scratch,
        restacked: false,
        moves: RigidMoves::default(),
        // Read once, here, rather than once per moved subtree: how a frame is being measured is
        // decided before the frame and must not change part-way through one.
        passes: split::current(),
    };
    pass.visit(root, descent, None, None);
    let moves = pass.moves;
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
    counter::set(Counter::BoxesLive, u64::from(store.box_capacity()));
    crate::invariants::check_if_enabled(store, hit);
    // A bulk rebuild of the index is deliberately *not* folded into `settled`. It rebuilds
    // painting order, and painting order is consulted where pixels are derived — inside this
    // frame's damage, after the rebuild has run. What a shift of the pixels outside the damage
    // rests on is narrower: that every divergence from a pure move is *in* the damage, and the
    // walk puts it there unconditionally — a fragment born this frame has no previous and is
    // damaged whole, a retired one is damaged where it was, a changed one at both — and an
    // insertion moves no surviving fragment relative to another.
    let _ = restacked;
    moves
}

/// The viewport a subtree is composed inside, taken from the document's own root box.
fn viewport_of(store: &LayoutStore, fallback: BoxKey) -> Size<DevicePx, Device> {
    let key = store.root().unwrap_or(fallback);
    store
        .layout_of(key)
        .map(|layout| layout.size)
        .unwrap_or(Size::new(DevicePx(0.0), DevicePx(0.0)))
}

/// What this pass's rigid moves add up to, for a caller deciding whether pixels already on the
/// screen can be shifted instead of drawn again.
///
/// A scroll moves every pixel of a scrollport, so the damage it raises is the whole port and the
/// emit walk reaches every fragment in it. That is only unavoidable if the pixels have to be
/// *derived* again — and they do not, when the frame moved one thing rigidly and changed nothing
/// else. This is what says whether the frame was that frame; it is a report about the walk and
/// decides nothing on its own, because whether the pixels may be shifted also depends on what else
/// is drawn over the region, which this walk never looks at.
#[derive(Clone, Copy, Debug)]
pub struct RigidMoves {
    /// Whether every pass's moves can stand for the frame.
    ///
    /// Held true by the walk itself — everything a walk does that a move cannot express lands in
    /// [`RigidMoves::beyond`] instead — and false only when a caller combines it with a pass that
    /// said otherwise.
    pub settled: bool,
    /// The offset the rigid moves took, when every one of them took the same one.
    ///
    /// `None` for a pass that moved nothing, and for one whose moves disagreed — two scrollers
    /// gliding at once, say, which is a frame no single shift can answer.
    pub by: Option<(f32, f32)>,
    /// Whether two moves disagreed, which is why `by` may be `None` after something moved.
    pub conflicted: bool,
    /// How many subtrees were moved.
    pub count: u32,
    /// Everything this pass damaged for a reason other than a subtree moving rigidly.
    ///
    /// A caller that answers the movement by translating pixels drops the movement's own damage and
    /// keeps this, because this is what the translation does not cover. It is a strict subset of
    /// the pass's damage set and is never a substitute for it: a caller that refuses the
    /// translation must use the whole set, or it will not draw where things moved.
    ///
    /// Speaks for the walk and not for the frame: what the frame damaged before any walk ran is
    /// the caller's to add, because only the caller knows which walk was the first.
    pub beyond: DamageSet,
}

impl Default for RigidMoves {
    fn default() -> Self {
        Self {
            settled: true,
            by: None,
            conflicted: false,
            count: 0,
            beyond: DamageSet::new(),
        }
    }
}

impl RigidMoves {
    /// Both passes' moves, for a frame that ran the walk more than once.
    ///
    /// A scroll delivered to a listener can re-render and relay out inside the same frame, so the
    /// frame's answer is every pass's answer together: it moved rigidly only if *each* pass did,
    /// and it moved by one vector only if they agree on it.
    #[must_use]
    pub fn and(mut self, other: Self) -> Self {
        self.settled &= other.settled;
        self.conflicted |= other.conflicted;
        match (self.by, other.by) {
            (_, None) => {}
            (None, Some(by)) => self.by = Some(by),
            (Some(held), Some(by)) if held == by => {}
            (Some(_), Some(_)) => {
                self.by = None;
                self.conflicted = true;
            }
        }
        self.count += other.count;
        self.beyond.absorb_set(&other.beyond);
        self
    }

    /// Records one subtree moving by `by`.
    fn moved(&mut self, by: (f32, f32)) {
        self.count += 1;
        if self.conflicted {
            return;
        }
        match self.by {
            None => self.by = Some(by),
            // Deliberately not a tolerance. Two subtrees that moved by *almost* the same vector
            // cannot both be answered by one shift of the pixels, and the caller's fallback is the
            // frame it would have drawn anyway.
            Some(held) if held == by => {}
            Some(_) => {
                self.by = None;
                self.conflicted = true;
            }
        }
    }
}

/// The device pixels a fragment's clip chain lets it draw in.
///
/// A newtype rather than a bare rectangle so that a caller cannot pass a fragment's own ink where
/// the region admitting it belongs — the two are both device-space rectangles about the same box
/// and mean opposite things.
#[derive(Clone, Copy, Debug)]
struct Admitted(Rect<DevicePx, Device>);

impl Admitted {
    /// The region that cuts nothing, for damage no chain can be shown to bound.
    fn everything() -> Self {
        Self(Rect::new(
            zgui_geom::Point::new(DevicePx(f32::MIN / 2.0), DevicePx(f32::MIN / 2.0)),
            zgui_geom::Size::new(DevicePx(f32::MAX), DevicePx(f32::MAX)),
        ))
    }

    /// `rect` cut to what is admitted, or `None` when none of it is.
    fn cut(self, rect: Rect<DevicePx, Device>) -> Option<Rect<DevicePx, Device>> {
        rect.intersection(self.0)
    }
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
    /// The reusable buffers the walk works in.
    scratch: &'a mut DiffScratch,
    /// Whether this walk produced a fragment that has no place in the painting order yet.
    restacked: bool,
    /// What the walk's rigid moves add up to.
    moves: RigidMoves,
    /// Whether a subtree that only moved is offset in one descent or in one descent per duty.
    passes: split::Passes,
}

impl<D: FrameDirty> Pass<'_, '_, D> {
    /// Damages a rectangle for a reason other than a subtree moving rigidly.
    ///
    /// Every absorb in this walk goes through either this or the pair in
    /// [`rigid`](crate::fragment::diff::rigid), and the split is the whole of what
    /// [`RigidMoves::only`] reports: a caller may shift pixels it already has only when nothing was
    /// composed, removed or repainted in place, and this is where "something was" is recorded.
    fn damage_beyond_a_move(&mut self, rect: Rect<DevicePx, Device>, admitted: Admitted) {
        let Some(rect) = admitted.cut(rect) else {
            return;
        };
        absorb(&mut self.moves.beyond, rect);
        absorb(self.damage, rect);
    }

    /// What a fragment's clip chain admits, in device pixels.
    ///
    /// Damage is what has to be *drawn again*, and a box cannot draw outside the chain it is drawn
    /// under: [`Scene::assign_order`](zgui_scene::Scene) refuses a primitive its chain admits
    /// nothing of, so a damage rectangle beyond the chain names pixels no frame will ever put
    /// anything into.
    ///
    /// This is not a refinement. It is the difference between damage bounded by the window and
    /// damage bounded by the *document*: a virtualised list stands two spacers in for the rows it
    /// did not build, and their border boxes are as tall as the whole list — a quarter of a million
    /// pixels for ten thousand rows, a hundred and fifty screens. Changing one of them is a real
    /// change owing real damage, and the damage it owes is the part of the scrollport it can be
    /// seen in.
    ///
    /// Resolved through the frame's own matrices rather than read flat, because a chain inside a
    /// transformed subtree interned its rectangles in that subtree's coordinates and the subtree
    /// moves without the links being interned again.
    fn admits(&self, clip: ClipId) -> Rect<DevicePx, Device> {
        let tables = &*self.tables;
        let spatial = &tables.spatial;
        tables.clips.bounds_placed(clip, &|id| spatial.resolve(id))
    }

    /// What a fragment's chain admits, as a region damage may be cut to.
    fn admitted(&self, clip: ClipId) -> Admitted {
        Admitted(self.admits(clip))
    }

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
        let inks_mark = self.scratch.child_inks.len();
        let mut ink = Rect::ZERO;
        for (slot, index) in fragments.clone().enumerate() {
            let frag = self.scratch.written[index];
            let Some(fragment) = self.store.fragment(frag) else {
                continue;
            };
            if slot == 0 {
                ink = fragment.ink;
            } else {
                self.scratch.child_inks.push(fragment.ink);
            }
        }

        let children_mark = self.scratch.children.len();
        self.scratch
            .children
            .extend_from_slice(&self.store.node(key).children);
        let children_end = self.scratch.children.len();
        let first_fragment = (!fragments.is_empty()).then(|| self.scratch.written[fragments.start]);
        let mut blending = placed.blends;
        let mut disjoint = true;
        let mut rigid = placed.rigid;
        let mut subtree_ink = self.scratch.child_inks[inks_mark..]
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
        // Deeper visits append their own regions past `children_end` and truncate them again, so
        // the indices walked here stay this box's children throughout.
        for index in children_mark..children_end {
            let child = self.scratch.children[index];
            let clean = self.can_skip(child, owed.node);
            let folded = match movement {
                _ if settled && clean => self.cached(child),
                Some(movement) if clean && self.can_translate(child) => {
                    self.translate(child, movement)
                }
                _ => self.visit(child, placed.descent, first_fragment, owed.node),
            };
            self.scratch.child_inks.push(folded.subtree_ink);
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
        let child_inks = &self.scratch.child_inks[inks_mark..];
        disjoint &= !child_inks.iter().any(|child| overlaps(ink, *child));
        disjoint &= pairwise_disjoint(child_inks);

        self.record_fold(fragments.clone(), subtree_ink, blending, disjoint, rigid);
        self.scratch.child_inks.truncate(inks_mark);
        self.scratch.children.truncate(children_mark);
        self.scratch.written.truncate(fragments.start);
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
    ///
    /// Returns the region of the scratch written list holding this box's fragments, in draw
    /// order. The caller truncates the region away when it is done with it.
    fn write_fragments(
        &mut self,
        key: BoxKey,
        placed: &Placed,
        parent: Option<FragKey>,
        owed: &Owed,
    ) -> (core::ops::Range<usize>, bool) {
        let kinds_mark = self.scratch.kinds.len();
        self.kinds_of(key);
        let count = self.scratch.kinds.len() - kinds_mark;
        // Names are kept for as long as the box keeps drawing the same things in the same
        // positions, because a name is what the hit index, the recorded painting and the previous
        // frame's damage all refer to. The first position that draws something different is where
        // reuse stops.
        let mut keep = 0;
        while keep < count {
            let kind = self.scratch.kinds[kinds_mark + keep];
            if self.store.reusable_fragment(key, keep, kind).is_none() {
                break;
            }
            keep += 1;
        }
        self.retire(key, keep);

        let written_mark = self.scratch.written.len();
        let mut moved = false;
        for slot in 0..count {
            let kind = self.scratch.kinds[kinds_mark + slot];
            let geometry = self.geometry_for(key, placed, slot, kind);
            let parent = if slot == 0 {
                parent
            } else {
                Some(self.scratch.written[written_mark])
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
            self.scratch.written.push(frag);
        }
        self.scratch.kinds.truncate(kinds_mark);
        (written_mark..self.scratch.written.len(), moved)
    }

    /// Drops every fragment of a box beyond the first `keep`, damaging what they covered.
    ///
    /// The rectangle a destroyed fragment occupied is nobody's ink from here on, so it is absorbed
    /// now or it is never absorbed at all and last frame's pixels stay on the screen.
    fn retire(&mut self, key: BoxKey, keep: usize) {
        let mark = self.scratch.stale.len();
        self.scratch
            .stale
            .extend(self.store.fragments_of_box(key).iter().skip(keep).copied());
        for index in mark..self.scratch.stale.len() {
            let frag = self.scratch.stale[index];
            // Where a destroyed piece *was*, so it is cut to nothing for the same reason a
            // vacated rectangle is: the chain it named is last frame's.
            let Some(gone) = self.store.fragment(frag).map(|it| it.subtree_ink) else {
                continue;
            };
            self.damage_beyond_a_move(gone, Admitted::everything());
        }
        self.scratch.stale.truncate(mark);
        // The index is not touched here: `truncate_fragments` records what it destroyed, and the
        // one drain after the walk unregisters every destroyed name by the same route whether its
        // box was visited or deleted.
        self.store.truncate_fragments(key, keep);
    }

    /// Appends what each fragment of this box draws, in the order it draws them, to the scratch
    /// kind list.
    fn kinds_of(&mut self, key: BoxKey) {
        let node = self.store.node(key);
        // A custom element before either: a registered implementation owns the box's painting
        // outright. Then a drawing before replaced content: an element carrying outlines is
        // drawing them itself, and nothing outside the document has been asked for a picture.
        let own = match node.painted {
            crate::node::kind::PaintedContent::Custom => FragmentKind::Custom,
            crate::node::kind::PaintedContent::Vector => FragmentKind::Vector,
            crate::node::kind::PaintedContent::Replaced => {
                let content = self
                    .store
                    .replaced(key)
                    .expect("a replaced discriminator has replaced metadata")
                    .id;
                FragmentKind::Replaced { content }
            }
            crate::node::kind::PaintedContent::Box => FragmentKind::Box,
        };
        self.scratch.kinds.push(own);
        if let Some(resolution) = self.store.inline_resolution(key) {
            for index in 0..resolution.lines.len() {
                self.scratch.kinds.push(FragmentKind::Line {
                    paragraph: resolution.paragraph,
                    line: u16::try_from(index).unwrap_or(u16::MAX),
                });
            }
        }
        // After the lines, so that editing the text inside a scrollport does not renumber the
        // slots its bars occupy — a name is what the hit index, the recorded painting and last
        // frame's damage all refer to, and reuse stops at the first slot that draws something else.
        if let Some(layout) = self.store.layout_of(key) {
            self.scratch
                .kinds
                .extend(crate::scroll_region::bar::kinds(&layout));
        }
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
                content_hash: 0,
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
                content_hash: 0,
            };
        }
        let line = match kind {
            FragmentKind::Line { line, .. } => line as usize,
            _ => 0,
        };
        let resolved = self
            .store
            .inline_resolution(key)
            .and_then(|resolution| resolution.lines.get(line));
        let rect = resolved
            .map(|line| build::line_rect(placed.content_box, line))
            .unwrap_or(Rect::ZERO);
        let content_hash = resolved.map_or(0, crate::inline::ellipsis::line_hash);
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
            content_hash,
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
        // What this fragment's chain admits *now*, which is the region its next ink can be drawn
        // in. Where it *was* is deliberately not cut to anything: the chain it was drawn under
        // belongs to a frame that is gone, and the node it named holds where its clipping box has
        // moved to since. Cutting the old rectangle to the new region is how a scrolled row's
        // vacated pixels get left on the screen — so the old rectangle is taken whole, which is an
        // over-approximation and is always safe.
        let admitted = self.admitted(next.clip);
        match change {
            Change::Identical => {
                // A colour change moves nothing, and neither does re-shaped text of the same
                // extent, so the comparison above cannot see either. Without this absorb the two
                // commonest frames there are — a hover repainting one button, and a counter whose
                // digit changed width for width — put nothing in the damage set at all and paint
                // nothing.
                // The repaint of a box that paints nothing is nothing — and marks land on it
                // all the same: a keyed list that spliced its children marks every retained row,
                // and each row's paintless container would otherwise stripe the port with damage
                // on every frame the window over it shifts. A style that *starts* painting flips
                // [`FragmentFlags::PAINTS_NOTHING`], which the geometry comparison reads, so such
                // a change lands in [`Change::Changed`] rather than here.
                let vacuous =
                    kind == FragmentKind::Box && next.flags.contains(FragmentFlags::PAINTS_NOTHING);
                if own.intersects(REPAINTS_IN_PLACE) && !vacuous {
                    self.damage_beyond_a_move(next.ink, admitted);
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
                    self.damage_beyond_a_move(previous.ink, Admitted::everything());
                }
                self.damage_beyond_a_move(next.ink, admitted);
                self.touch_hit(frag, change);
            }
            Change::Changed => {
                self.dirty
                    .mark(owed.node, Dirty::REPAINT | Dirty::REHIT | self.a11y(node));
                // A paintless box that moved rigidly, or stood still while its inner boxes
                // repositioned, owes the frame no damage of its own. Repaint marks do not hold
                // it back: a repaint of nothing is nothing, and a style that *starts* painting
                // flips [`FragmentFlags::PAINTS_NOTHING`], which the comparison reads on both
                // sides.
                let repositioned = kind == FragmentKind::Box
                    && previous
                        .as_ref()
                        .is_some_and(|previous| repositioned_within(previous, next));
                if !repositioned {
                    if let Some(previous) = &previous {
                        self.damage_beyond_a_move(previous.ink, Admitted::everything());
                    }
                    self.damage_beyond_a_move(next.ink, admitted);
                }
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
        fragments: core::ops::Range<usize>,
        subtree_ink: Rect<DevicePx, Device>,
        blending: bool,
        disjoint: bool,
        rigid: bool,
    ) {
        for index in fragments {
            let frag = self.scratch.written[index];
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
