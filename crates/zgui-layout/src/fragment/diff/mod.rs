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

use core::cell::Cell;

use zgui_bits::{DamageSet, Dirty};
use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Rect, Size};
use zgui_profile::{Counter, counter};
use zgui_scene::ClipId;

use crate::fragment::build::{Descent, Placed, Tables};
use crate::fragment::hit::{HitIndex, HitOrder};
use crate::fragment::{FragKey, FragmentFlags, FragmentKind, build};
use crate::tree::store::LayoutStore;

mod damage;
mod dirty;
mod geometry;
mod rematrix;
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
    // The poison offset is a whole number, so rounding against it rounds as at the origin.
    let poison = zgui_scene::spatial::poison::origin();
    let seed = crate::round::snap::seed(store, root);
    let seed = (seed.0 + poison, seed.1 + poison);
    let descent = Descent {
        layout: seed,
        rounded: (seed.0.round(), seed.1.round()),
        ..Descent::root(viewport_of(store, root), tables.spatial.document())
    };
    let anchors = Anchors::of(tables, descent.anchor);
    let scale = tables.device.scale;
    // Every piece of a box that left the tree before this walk began. The walk descends the boxes
    // that are there now, so it never reaches one of these, and the rectangle a departed piece
    // covered is nobody's ink from here on: it is absorbed here or last frame's pixels stay on the
    // screen. [`Pass::retire`] does the same for a box that kept its name and lost a piece.
    //
    // A box that painted nothing leaves nothing behind of its own: the pieces below it left with
    // it and are in this list too. Its subtree's rectangle would add only what lay between them —
    // for the band an overlay is mounted on, the whole window.
    let vacated: Vec<(Rect<DevicePx, Device>, zgui_scene::SpatialId)> = store
        .retired_fragments()
        .iter()
        .filter_map(|frag| store.fragment(*frag))
        .filter(|piece| !paints_nothing_itself(piece.kind, piece.flags))
        .map(|piece| (piece.subtree_ink, piece.anchor))
        .collect();
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
        renumber: false,
        born_order: None,
        anchors,
        moves: RigidMoves::default(),
        // Read once, here, rather than once per moved subtree: how a frame is being measured is
        // decided before the frame and must not change part-way through one.
        passes: split::current(),
        refused: None,
        chase: Chase::default(),
        resolved: [Cell::new(None), Cell::new(None)],
    };
    for (gone, anchor) in vacated {
        // Taken whole rather than cut to anything: the chain it was drawn under belongs to a frame
        // that is gone.
        pass.damage_beyond_a_move(pass.device_before(gone, anchor), Admitted::everything());
    }
    pass.visit(root, descent, None, None, (0, HitOrder::MAX));
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
    //
    // A rebreak is marked only with a relayout, and the layout pass before this one serviced
    // both. Nothing else retires it, and a rebreak left set makes every later delivery in every
    // frame restyle and lay out the document again.
    let root_source = pass.store.node(root).source;
    pass.dirty.retire(root_source, ENTERS | Dirty::REBREAK);
    let restacked = pass.restacked || pass.renumber;
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
        let owner = zgui_scene::PropertyOwner::of(gone);
        tables.spatial.release(owner);
        tables.spatial.release(owner.content());
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

/// The matrices that put a walk's anchored rectangles on the device.
///
/// Resolved once per walk for the anchor nearly every fragment names, so the common case costs a
/// comparison of two names and no arithmetic.
#[derive(Clone, Copy, Debug)]
struct Anchors {
    /// The document's root space.
    root: zgui_scene::SpatialId,
    /// What the root space resolves to in the tree this walk writes, when that is not the
    /// identity.
    now: Option<zgui_geom::Matrix4>,
    /// What it resolved to when the standing fragments were composed, when that is not the
    /// identity.
    before: Option<zgui_geom::Matrix4>,
}

impl Anchors {
    /// The answers for `root`.
    fn of(tables: &Tables<'_>, root: zgui_scene::SpatialId) -> Self {
        let now = tables.spatial.resolve(root);
        let before = tables.drawn.get(root).copied().or(now);
        let moving = |matrix: Option<zgui_geom::Matrix4>| {
            matrix.filter(|matrix| *matrix != zgui_geom::Matrix4::IDENTITY)
        };
        Self {
            root,
            now: moving(now),
            before: moving(before),
        }
    }
}

/// `rect` through `matrix`, or as it is when there is none.
fn through(
    matrix: Option<zgui_geom::Matrix4>,
    rect: Rect<DevicePx, Device>,
) -> Rect<DevicePx, Device> {
    match matrix {
        Some(matrix) => crate::fragment::transform::transformed_bounds(&matrix, rect),
        None => rect,
    }
}

/// Where a rectangle in a box's own space is drawn in its anchor's space, through the box's
/// matrix.
fn on_device(placed: &Placed, rect: Rect<DevicePx, Device>) -> Rect<DevicePx, Device> {
    match placed.descent.matrix {
        Some(matrix) => crate::fragment::transform::transformed_bounds(&matrix, rect),
        None => rect,
    }
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
    /// How many subtrees were moved: one for each content space a scroll wrote.
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

    /// Records one content space moving by `by`.
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

/// What a box held before this walk, read off its first fragment before the walk rewrites it.
#[derive(Clone, Copy, Debug)]
struct Held {
    /// The coordinate system its own content was drawn in.
    transform: Option<zgui_scene::SpatialId>,
    /// A fingerprint of what that system resolved to.
    transform_hash: u64,
    /// Its flags.
    flags: FragmentFlags,
    /// The union of its ink and everything below it.
    subtree_ink: Rect<DevicePx, Device>,
    /// Its padding box, which is a scroll container's port.
    padding_box: Rect<DevicePx, Device>,
    /// The coordinate system its ink was measured in.
    anchor: zgui_scene::SpatialId,
}

impl Held {
    /// What `fragment` holds, with what `store` holds beside it.
    fn of(store: &LayoutStore, fragment: &crate::fragment::Fragment) -> Self {
        Self {
            transform: fragment.transform,
            transform_hash: store.fragment_extra(fragment).transform_hash,
            flags: fragment.flags,
            subtree_ink: fragment.subtree_ink,
            padding_box: fragment.padding_box(),
            anchor: fragment.anchor,
        }
    }
}

/// The content space a scroll container draws its children in.
#[derive(Clone, Copy, Debug)]
struct ContentSpace {
    /// Its name.
    id: zgui_scene::SpatialId,
    /// How far its matrix moved since the last walk, when it held one then and it moved.
    moved: Option<(f32, f32)>,
}

/// What one box's subtree reported to the box above it.
#[derive(Clone, Copy, Debug)]
struct Folded {
    /// The union of this box's ink and everything below it.
    subtree_ink: Rect<DevicePx, Device>,
    /// Whether anything at or below this box blends with what is behind it.
    blending: bool,
    /// Whether anything at or below this box reads what is behind it: it blends with it or
    /// filters it.
    reads: bool,
    /// Whether nothing in this subtree overlaps anything else in it.
    disjoint: bool,
    /// Whether every piece in this subtree moves by the same vector as the box above it.
    rigid: bool,
    /// The range of hit-order keys the subtree spans, `(MAX, 0)` when it indexes nothing.
    order: (HitOrder, HitOrder),
    /// The coordinate system [`Folded::subtree_ink`] is measured in, or `None` for a subtree with
    /// no piece.
    ///
    /// The anchor the box above hands its children, except for a fixed box inside a content
    /// space: it measures its ink where it hangs from.
    anchor: Option<zgui_scene::SpatialId>,
    /// The [`FragmentFlags::BOUND`] flags of the subtree.
    bound: FragmentFlags,
}

impl Folded {
    /// The fold of a subtree with nothing in it.
    fn empty() -> Self {
        Self {
            subtree_ink: Rect::ZERO,
            blending: false,
            reads: false,
            disjoint: true,
            rigid: true,
            order: (HitOrder::MAX, 0),
            anchor: None,
            bound: FragmentFlags::EMPTY,
        }
    }
}

/// Which boxes below a content space that moved in this frame are visited all the same.
///
/// A scroll that writes a content space leaves the boxes in it as they are. Two kinds of box do
/// not move with the content: a sticky box, whose shift is measured against the port, and a fixed
/// box, which takes no scroll offset. Each is composed again, and so is the path down to it, so the
/// subtree inks folded above it say where it is.
#[derive(Clone, Copy, Debug, Default)]
struct Chase {
    /// How far the innermost content space that moved in this frame moved, on the device.
    by: Option<(f32, f32)>,
    /// Whether the sticky boxes of that content space's port are visited.
    sticky: bool,
    /// Whether the fixed boxes below are visited.
    fixed: bool,
}

impl Chase {
    /// Whether a child whose first piece holds the flags `bound` is visited.
    fn wants(self, bound: FragmentFlags) -> bool {
        (self.sticky && bound.contains(FragmentFlags::STICKY_BELOW))
            || (self.fixed && bound.contains(FragmentFlags::FIXED_BELOW))
    }

    /// What the children of `placed` are visited under, when its content space moved by `moved`
    /// in this frame.
    ///
    /// `previous` is the box's flags from its last composition. Only a box whose subtree then held
    /// a sticky or fixed box has one to visit. A box with no previous flags checks every child.
    fn below(
        self,
        placed: &Placed,
        moved: Option<(f32, f32)>,
        previous: Option<FragmentFlags>,
    ) -> Self {
        let chase = if let Some(by) = moved {
            Self {
                by: Some(by),
                sticky: true,
                fixed: true,
            }
        } else if placed.fixed {
            // Nothing below a fixed box moves with the content around it.
            Self::default()
        } else if placed.scrolls {
            // The sticky boxes below are sticky against this port, which did not scroll.
            Self {
                sticky: false,
                ..self
            }
        } else {
            self
        };
        let Some(flags) = previous else {
            return chase;
        };
        // A scroll container stops STICKY_BELOW and keeps STICKY_IN_PORT in its place.
        let sticky = if moved.is_some() {
            FragmentFlags::STICKY_IN_PORT
        } else {
            FragmentFlags::STICKY_BELOW
        };
        Self {
            sticky: chase.sticky && flags.contains(sticky),
            fixed: chase.fixed && flags.contains(FragmentFlags::FIXED_BELOW),
            ..chase
        }
    }
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
    /// Whether the painting order itself moved under this walk, so that the keys the index
    /// carries have to be handed out again.
    restacked: bool,
    /// Whether a newborn fragment found no room between its neighbours' order keys.
    ///
    /// Answered by handing every key out again once the walk is done, which is the rebuild the
    /// index already knows how to do; nothing queries the index before then.
    renumber: bool,
    /// The order key the fragment about to be born takes, handed from the box that placed it to
    /// the index write that files it.
    born_order: Option<HitOrder>,
    /// What puts the walk's anchored rectangles on the device.
    anchors: Anchors,
    /// What the walk's rigid moves add up to.
    moves: RigidMoves,
    /// Whether a subtree that only moved is offset in one descent or in one descent per duty.
    passes: split::Passes,
    /// The box being composed again without a content space, because a fixed box below it hangs
    /// from a transformed or sticky box the space would move.
    refused: Option<BoxKey>,
    /// Which children of the box being visited are visited because a content space above moved.
    chase: Chase,
    /// The last two coordinate systems [`Pass::resolve`] answered, kept while the tree holds
    /// still.
    resolved: [Cell<Option<Resolved>>; 2],
}

/// One coordinate system's matrix, under the spatial tree's write stamp it was resolved at.
#[derive(Clone, Copy, Debug)]
struct Resolved {
    /// The coordinate system.
    id: zgui_scene::SpatialId,
    /// [`PropertyTree::writes`](zgui_scene::PropertyTree::writes) when it was resolved.
    stamp: u64,
    /// What [`SpatialTree::resolve`](zgui_scene::SpatialTree::resolve) gave.
    matrix: Option<zgui_geom::Matrix4>,
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

    /// `rect`, measured in `anchor` as this walk composes it, in device pixels.
    ///
    /// For new ink. Damage is measured on the device, and an anchored rectangle is device pixels
    /// only through its anchor's matrix.
    fn device_now(
        &self,
        rect: Rect<DevicePx, Device>,
        anchor: zgui_scene::SpatialId,
    ) -> Rect<DevicePx, Device> {
        if anchor == self.anchors.root {
            return through(self.anchors.now, rect);
        }
        through(self.resolve(anchor), rect)
    }

    /// What `id` resolves to in the tree this walk writes.
    ///
    /// The answer [`SpatialTree::resolve`](zgui_scene::SpatialTree::resolve) gives, in every bit.
    /// The last two answers are kept under the tree's
    /// [write stamp](zgui_scene::PropertyTree::writes): the rows of one content space name the
    /// same anchor, and their clip chains the space above it, so each is resolved once.
    fn resolve(&self, id: zgui_scene::SpatialId) -> Option<zgui_geom::Matrix4> {
        let spatial = &*self.tables.spatial;
        let stamp = spatial.writes();
        let [last, before] = &self.resolved;
        let kept =
            |held: Option<Resolved>| held.filter(|held| held.id == id && held.stamp == stamp);
        let matrix = match kept(last.get()).or_else(|| kept(before.get())) {
            Some(held) => held.matrix,
            None => {
                let matrix = spatial.resolve(id);
                before.set(last.get());
                last.set(Some(Resolved { id, stamp, matrix }));
                matrix
            }
        };
        debug_assert!(
            {
                let bits = |matrix: Option<zgui_geom::Matrix4>| {
                    matrix.map(|matrix| matrix.columns.map(|column| column.map(f32::to_bits)))
                };
                bits(matrix) == bits(spatial.resolve(id))
            },
            "the kept matrix of {id:?} is not the one the tree resolves"
        );
        matrix
    }

    /// `rect`, measured in `anchor` as the standing fragments were composed, in device pixels.
    ///
    /// For old ink: where a piece was is where the last published matrices put it.
    fn device_before(
        &self,
        rect: Rect<DevicePx, Device>,
        anchor: zgui_scene::SpatialId,
    ) -> Rect<DevicePx, Device> {
        if anchor == self.anchors.root {
            return through(self.anchors.before, rect);
        }
        let tables = &*self.tables;
        let matrix = tables
            .drawn
            .get(anchor)
            .copied()
            .or_else(|| tables.spatial.resolve(anchor));
        through(matrix, rect)
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
        self.tables
            .clips
            .bounds_placed(clip, &|id| self.resolve(id))
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
        between: (HitOrder, HitOrder),
    ) -> Folded {
        counter::bump(Counter::NodesVisited);
        counter::bump(Counter::FragmentBoxesVisited);
        let Some(mut placed) = build::place(self.store, self.tables, key, from) else {
            return Folded::empty();
        };
        let owed = self.owed_by(key, generator);
        let own = owed.own;
        // Read before the write replaces them: whether the matrix this box's children are drawn
        // through is the one they were composed under, and whether the coordinate system they are
        // drawn in is still the one they name.
        let previous = self
            .store
            .fragments_of_box(key)
            .first()
            .and_then(|frag| self.store.fragment(*frag))
            .map(|fragment| Held::of(self.store, fragment));
        let content = self.content_space(key, &placed, previous, own);
        let transform_stable =
            previous.is_some_and(|held| held.transform_hash == placed.transform_hash);
        // A box that starts or stops drawing its children in a content space hands them another
        // coordinate system, exactly as a box that takes or gives back a space of its own does. So
        // does a box whose anchor changed under a name that stayed: its children measure their
        // ink in that anchor.
        let space_stable = previous.is_some_and(|held| {
            held.transform == placed.transform
                && held.anchor == placed.anchor
                && held.flags.contains(FragmentFlags::SCROLLS_CONTENT) == content.is_some()
        });
        // The marks that decide whether the children were composed against what this box hands
        // down. A scroll of a box that draws its children in a content space moves that space
        // and nothing the children are composed against.
        let mut leading = own;
        // How far the content space moved in this frame, when the children stay where they are in
        // it.
        let mut content_moved = None;
        if let Some(content) = content {
            placed.flags = placed.flags.union(FragmentFlags::SCROLLS_CONTENT);
            placed.descent = placed.content_descent(content.id);
            leading = own.difference(Dirty::SCROLL);
            if space_stable && let Some(by) = content.moved {
                self.scroll_content(&placed, previous, by);
                content_moved = Some(by);
            }
        }
        // Where this box is: below a content space that moved in this frame, or not.
        let here = self.chase;
        let state = self.store.state_mut(key);
        state.snapped = placed.snapped;
        // Read before it is replaced: whether this box's size is what its children were composed
        // against decides whether a child that reads the size — a sticky one, through the
        // scrollport and the containing block — can be left alone.
        let size_stable = state.composed.size == state.unrounded.size;
        state.composed = state.unrounded;
        // Read before it is replaced: the difference between the two is how far this box's contents
        // have moved since they were last composed, which is the whole input to the offsetting path.
        let previous_shift = state.composed_shift;
        state.composed_shift = placed.descent.shift;
        let movement = self.movement(&placed, previous_shift);

        // A stacking change reorders what this box's children are painted after; the keys the
        // index carries for them are then numbered for an order that no longer holds.
        if own.contains(Dirty::RESTACK) {
            self.restacked = true;
        }
        let (fragments, moved) = self.write_fragments(key, &placed, parent, &owed, between);

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

        let mut order = (HitOrder::MAX, 0);
        for index in fragments.clone() {
            if let Some(held) = self.hit.order_of(self.scratch.written[index]) {
                order = (order.0.min(held), order.1.max(held));
            }
        }
        // Where the next newborn goes: after everything painted so far, which is this box's own
        // pieces once they are written, or whatever came before the box when it has none.
        let mut cursor = if order.1 > 0 { order.1 } else { between.0 };

        // The children are walked in painting order. A box born anywhere below one of them is
        // placed between the order keys of what was walked before it and the next sibling, so
        // those have to be its neighbours in painting order.
        let children_mark = self.scratch.children.len();
        self.scratch.children.extend_from_slice(
            &crate::fragment::stacking::children_in_paint_order(self.store, key),
        );
        let children_end = self.scratch.children.len();
        let first_fragment = (!fragments.is_empty()).then(|| self.scratch.written[fragments.start]);
        let mut blending = placed.blends;
        let mut below = false;
        let mut disjoint = true;
        let mut rigid = placed.rigid;
        // The flags the children's subtrees fold up, before this box adds its own.
        let mut children_bound = FragmentFlags::EMPTY;
        // What puts a child's ink, measured in the content space, into this box's anchor.
        let into_anchor =
            content.map(|_| Size::new(DevicePx(-placed.offset.0), DevicePx(-placed.offset.1)));
        self.chase = here.below(&placed, content_moved, previous.map(|held| held.flags));
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
        //
        // What a child's composition reads off this box is its rounded origin, the shift above it
        // and the matrix it is drawn through — and not the box's size, unless the child is sticky
        // or is itself a different shape. So the children are left alone when those three stood
        // still, whatever happened to the box's own pieces: a column that grew because one row
        // did has moved none of the rows above it.
        let settled = !leading.intersects(ENTERS)
            && placed.descent.shift == previous_shift
            && movement.is_none()
            && (!moved || (placed.descent.origin_stable && transform_stable));
        // The same three inputs, with the matrix the one that moved: a pan or a zoom written on
        // this box. A clean child is then carried onto the new matrix rather than composed again,
        // because everything it holds but its device ink is measured in a space the write did not
        // move. See [`rematrix`].
        let carried = !transform_stable
            && !leading.intersects(Dirty::RESTACK | Dirty::SCROLL)
            && placed.descent.shift == previous_shift
            && movement.is_none()
            && (!moved || placed.descent.origin_stable)
            && rematrix::keeps_rectangles(placed.descent.matrix);
        // Deeper visits append their own regions past `children_end` and truncate them again, so
        // the indices walked here stay this box's children throughout.
        for index in children_mark..children_end {
            let child = self.scratch.children[index];
            let clean = self.can_skip(child, owed.node);
            let next = self.next_indexed_after(index + 1, children_end, between.1);
            let child_between = (cursor, next);
            let folded = match movement {
                // Every shortcut below keeps the name of the coordinate system a child's pieces are
                // drawn in. A box that gave its own system back or took one, or whose parent's
                // system changed name, hands its children a different name, so they are composed
                // again and take it. A kept name draws them in the wrong system, and a name given
                // back stops resolving when the frame ends.
                _ if !space_stable => self.visit(
                    child,
                    placed.descent,
                    first_fragment,
                    owed.node,
                    child_between,
                ),
                // A sticky or fixed box below a content space that moved, or the path down to
                // one: it does not move with the content.
                _ if self.chases(child) => self.visit(
                    child,
                    placed.descent,
                    first_fragment,
                    owed.node,
                    child_between,
                ),
                _ if settled && clean && (size_stable || self.can_translate(child)) => {
                    self.cached(child)
                }
                _ if carried
                    && clean
                    && size_stable
                    && self.drawn_under(child, placed.descent.clip) =>
                {
                    self.rematrix(child, placed.descent.matrix, placed.descent.fixed.matrix)
                }
                Some(movement) if clean && self.can_translate(child) => {
                    self.translate(child, movement)
                }
                _ => match self.layout_shift(child, &placed, previous_shift, owed.node) {
                    Some((shift, snapped)) => {
                        // Adopted before the carry, so the walk finds every box below composed
                        // from the result it holds — this one included.
                        let state = self.store.state_mut(child);
                        state.composed = state.unrounded;
                        state.snapped = snapped;
                        if shift.by == (0.0, 0.0) && shift.scrolled == (0.0, 0.0) {
                            self.cached(child)
                        } else {
                            self.translate(child, shift)
                        }
                    }
                    None => self.visit(
                        child,
                        placed.descent,
                        first_fragment,
                        owed.node,
                        child_between,
                    ),
                },
            };
            let mut folded = folded;
            match folded.anchor {
                // A fixed box inside a content space measures its ink where it hangs from.
                Some(anchor) if anchor != placed.descent.anchor => {
                    folded.subtree_ink = self.reanchor(folded.subtree_ink, anchor, placed.anchor);
                }
                _ => {
                    if let Some(by) = into_anchor {
                        folded.subtree_ink = folded.subtree_ink.translate(by);
                    }
                }
            }
            self.scratch.child_inks.push(folded.subtree_ink);
            blending |= folded.blending;
            below |= folded.reads;
            disjoint &= folded.disjoint;
            rigid &= folded.rigid;
            children_bound = children_bound.union(folded.bound);
            subtree_ink = subtree_ink.union(folded.subtree_ink);
            if folded.order.1 > 0 {
                cursor = cursor.max(folded.order.1);
                order = (order.0.min(folded.order.0), order.1.max(folded.order.1));
            }
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

        self.chase = here;
        // A scroll container is the port of the sticky boxes below it, so it stops STICKY_BELOW.
        // It keeps STICKY_IN_PORT on itself, which does not fold up. A fixed box stops every bound
        // flag except FIXED_UNDER_SCROLLED.
        let mut bound = children_bound;
        let mut folded = FragmentFlags::EMPTY;
        if placed.scrolls {
            if bound.contains(FragmentFlags::STICKY_BELOW) {
                folded = FragmentFlags::STICKY_IN_PORT;
            }
            bound = bound.without(FragmentFlags::STICKY_BELOW);
        }
        if placed.fixed {
            bound = bound.intersection(FragmentFlags::FIXED_UNDER_SCROLLED);
        }
        let bound = bound.union(placed.bound);
        folded = folded.union(bound);
        if blending {
            folded = folded.union(FragmentFlags::HAS_BLENDING_DESCENDANT);
        }
        if below {
            folded = folded.union(FragmentFlags::READS_BACKDROP_BELOW);
        }
        // Every piece belongs to this box, which holds the range for all of them.
        if !fragments.is_empty() {
            self.store.set_subtree_order(key, order);
        }
        self.record_fold(fragments.clone(), subtree_ink, folded, disjoint, rigid);
        self.scratch.child_inks.truncate(inks_mark);
        self.scratch.children.truncate(children_mark);
        self.scratch.written.truncate(fragments.start);
        // A fixed box that hangs from a transformed or sticky box in the content space moves with
        // the space. A box below that took that shape in this frame was composed in it all the same.
        // So the box is composed again without one, which composes every child again in the
        // box's own space. The entries the first composition carried are settled first, because
        // the second one may write the same entries again.
        if content.is_some() && children_bound.contains(FragmentFlags::FIXED_UNDER_SCROLLED) {
            self.hit.settle();
            let outer = self.refused.replace(key);
            let again = self.visit(key, from, parent, generator, between);
            self.refused = outer;
            return again;
        }
        // The frame may answer the content's move by moving the port's pixels. A box that does
        // not move with the content then owes where those pixels put it, and a fixed box also
        // owes where it is.
        if let Some(by) = here.by
            && (placed.fixed || moved)
        {
            let admitted = self.admitted(placed.clip);
            if let Some(held) = previous {
                let was = self.device_before(held.subtree_ink, held.anchor);
                let carried = was.translate(Size::new(DevicePx(by.0), DevicePx(by.1)));
                self.damage_beyond_a_move(carried, admitted);
            }
            if placed.fixed {
                self.damage_beyond_a_move(self.device_now(subtree_ink, placed.anchor), admitted);
            }
        }
        Folded {
            subtree_ink,
            blending,
            reads: below || placed.blends || placed.flags.contains(FragmentFlags::FILTERS_BACKDROP),
            disjoint,
            rigid,
            order,
            anchor: Some(placed.anchor),
            bound,
        }
    }

    /// Whether `child` is visited because a content space above moved, read off the
    /// [`FragmentFlags::BOUND`] flags its subtree folded up when it was last composed.
    fn chases(&self, child: BoxKey) -> bool {
        if !self.chase.sticky && !self.chase.fixed {
            return false;
        }
        self.store
            .fragments_of_box(child)
            .first()
            .and_then(|frag| self.store.fragment(*frag))
            .is_some_and(|fragment| self.chase.wants(fragment.flags))
    }

    /// `rect`, measured in `from`, measured in `to` instead, through this walk's matrices.
    fn reanchor(
        &self,
        rect: Rect<DevicePx, Device>,
        from: zgui_scene::SpatialId,
        to: zgui_scene::SpatialId,
    ) -> Rect<DevicePx, Device> {
        if rect.is_empty() || from == to {
            return rect;
        }
        let (Some(from), Some(to)) = (self.resolve(from), self.resolve(to)) else {
            return rect;
        };
        if let (Some(from), Some(to)) = (from.plane_translation(), to.plane_translation()) {
            return rect.translate(Size::new(DevicePx(from.0 - to.0), DevicePx(from.1 - to.1)));
        }
        match to.invert() {
            Some(into) => crate::fragment::transform::transformed_bounds(&from.then(&into), rect),
            None => rect,
        }
    }

    /// The content space `key` draws its children in this frame, if it draws them in one.
    ///
    /// A scroll container takes one (see [`scroll_nodes`](crate::fragment::scroll_nodes)) when it
    /// clips its children, nothing between it and its anchor is transformed, no fixed box below
    /// hangs from a transformed or sticky box below it (see
    /// [`FragmentFlags::FIXED_UNDER_SCROLLED`]), and it is composed for the first time, held a
    /// content space already or scrolls in this frame. A new box takes the space at once, because
    /// its children are composed in this frame anyway: its first scroll is then a write of the
    /// space. A box that was composed without one takes it at its first scroll, because taking it
    /// composes and paints every child again once. A new box does not know yet whether a fixed box
    /// below keeps it out; the walk composes it again without the space when one does.
    ///
    /// The space is written here: its parent is the box's own space and its matrix is the box's
    /// scroll offset. A box that no longer qualifies gives its space back.
    fn content_space(
        &mut self,
        key: BoxKey,
        placed: &Placed,
        previous: Option<Held>,
        own: Dirty,
    ) -> Option<ContentSpace> {
        let owner = zgui_scene::PropertyOwner::of(key).content();
        let held = previous.is_some_and(|held| held.flags.contains(FragmentFlags::SCROLLS_CONTENT));
        let wanted = self.refused != Some(key)
            && placed.scrolls
            && placed.flags.contains(FragmentFlags::CLIPS_CHILDREN)
            && placed.descent.matrix.is_none()
            && previous.is_none_or(|previous| {
                !previous.flags.contains(FragmentFlags::FIXED_UNDER_SCROLLED)
                    && (held || own.contains(Dirty::SCROLL))
            });
        if !wanted {
            if held {
                self.tables.spatial.release(owner);
            }
            return None;
        }
        let local = zgui_geom::Matrix4::translation(-placed.offset.0, -placed.offset.1, 0.0);
        let spatial = &mut *self.tables.spatial;
        let before = spatial
            .of(owner)
            .and_then(|id| spatial.get(id))
            .map(|node| node.local);
        let id = spatial.establish(
            owner,
            zgui_scene::SpatialNode {
                parent: Some(placed.descent.spatial),
                local,
                anchoring: zgui_scene::Anchoring::Content,
            },
        );
        // How far the content moved since the space was last written, read off the two matrices.
        // A space established in this frame has no movement: its children are composed again.
        let moved = before.filter(|_| held).and_then(|before| {
            let by = (
                local.columns[3][0] - before.columns[3][0],
                local.columns[3][1] - before.columns[3][1],
            );
            (by != (0.0, 0.0)).then_some(by)
        });
        Some(ContentSpace { id, moved })
    }

    /// Records a scroll that moved only a content space.
    ///
    /// The scroll moved every pixel of the port and nothing else, so the port is the move's damage
    /// and the move is recorded once, for the whole container. Settled children are not visited,
    /// so nothing else is damaged on their behalf. A child born or changed in the same frame is
    /// composed and damages what it draws. The hit index is told that what is under a point
    /// changed, because no entry did.
    fn scroll_content(&mut self, placed: &Placed, previous: Option<Held>, by: (f32, f32)) {
        self.moves.moved(by);
        if let Some(held) = previous {
            absorb(
                self.damage,
                self.device_before(held.padding_box, held.anchor),
            );
        }
        let port = self.device_now(placed.padding_box, placed.anchor);
        if let Some(port) = self.admitted(placed.descent.clip).cut(port) {
            absorb(self.damage, port);
        }
        self.hit.note_space_moved();
    }

    /// The first order key painted after the child at `from` among this box's children, or
    /// `after` when no later sibling indexes anything.
    ///
    /// Read off the siblings' folded ranges from the last pass. A sibling about to be composed
    /// again keeps a range that is still a sound upper bound: whatever it becomes is placed
    /// between the cursor and its own next sibling when its turn comes.
    fn next_indexed_after(&self, from: usize, end: usize, after: HitOrder) -> HitOrder {
        for index in from..end {
            let sibling = self.scratch.children[index];
            if let Some(&frag) = self.store.fragments_of_box(sibling).first()
                && self.store.fragment(frag).is_some()
            {
                let first = self.store.subtree_order(sibling).0;
                if first != HitOrder::MAX {
                    return first;
                }
            }
        }
        after
    }

    /// How far this box's contents have moved, when that is all that happened to them.
    ///
    /// The shift the contents are composed against changes with a sticky offset, and with the
    /// scroll offset of a container that keeps its children in its own space. Each rigid child is
    /// then carried as a layout move: the frame draws where it was and where it is, and moves no
    /// pixels for it.
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
            placed.descent.origin_stable && placed.descent.matrix.is_none() && by != (0.0, 0.0);
        usable.then_some(rigid::Move {
            by,
            scrolled: by,
            clip: placed.descent.clip,
            shift: placed.descent.shift,
        })
    }

    /// Whether layout merely carried a child somewhere else, and by how much.
    ///
    /// The composing path exists for a box whose result changed; a box whose result differs from
    /// what its fragments were composed from *only in where it is* has fragments that are right
    /// in every respect but one, and everything below it is right in every respect. That is the
    /// row after the row that grew, and there are as many of them as the list has rows.
    ///
    /// The claim needs the subtree to owe nothing, to be rigid — nothing sticky, fixed or
    /// transformed below, since those read more than their own origin — and to be drawn through
    /// no matrix, so that a vector in this space is a vector on the device. And it needs the
    /// offset to be a whole number of device pixels with the far edges moving by the same one:
    /// snapping rounds cumulative absolute edges, and an offset that is not whole can round some
    /// edge below the other way, which is a box of a different shape and not a moved one.
    ///
    /// Answers the snapped result beside the move, which the caller adopts before carrying.
    /// `previous_shift` is the shift the child was last composed under: what the offsets above it
    /// scrolled since then is part of the move as well.
    fn layout_shift(
        &self,
        child: BoxKey,
        placed: &Placed,
        previous_shift: (f32, f32),
        generator: Option<zgui_dom::NodeKey>,
    ) -> Option<(rigid::Move, cephal::Layout)> {
        let owed = self.owed_by(child, generator);
        if owed.own.intersects(ENTERS) || owed.subtree.intersects(ENTERS) {
            return None;
        }
        if placed.descent.matrix.is_some() || !self.can_translate(child) {
            return None;
        }
        let state = self.store.state(child)?;
        let (new, old) = (state.unrounded, state.composed);
        // Everything but where it is. The engine's `order` is left out on purpose: it is the
        // child's index among its siblings, which a reorder moves for every row, and painting
        // order is derived from the box tree rather than read from it.
        let same_shape = new.size == old.size
            && new.border == old.border
            && new.padding == old.padding
            && new.margin == old.margin
            && new.scrollable_overflow_rect == old.scrollable_overflow_rect
            && new.scrollbar_size == old.scrollbar_size;
        if !same_shape {
            return None;
        }
        let first = *self.store.fragments_of_box(child).first()?;
        let fragment = self.store.fragment(first)?;
        if fragment.kind != FragmentKind::Box {
            return None;
        }
        let (snapped, edges) =
            crate::round::snap::place(new, placed.descent.layout, placed.descent.rounded);
        let shift = placed.descent.shift;
        let held = fragment.border_box;
        let dx = edges.left + shift.0 - held.origin.x.0;
        let dy = edges.top + shift.1 - held.origin.y.0;
        if dx.fract() != 0.0 || dy.fract() != 0.0 {
            return None;
        }
        let far_x = edges.right + shift.0 - (held.origin.x.0 + held.size.width.0);
        let far_y = edges.bottom + shift.1 - (held.origin.y.0 + held.size.height.0);
        if far_x != dx || far_y != dy {
            return None;
        }
        Some((
            rigid::Move {
                by: (dx, dy),
                scrolled: (shift.0 - previous_shift.0, shift.1 - previous_shift.1),
                clip: placed.descent.clip,
                shift,
            },
            snapped,
        ))
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

    /// Whether `child`'s own piece is drawn under `clip`, the chain its parent now hands down.
    fn drawn_under(&self, child: BoxKey, clip: ClipId) -> bool {
        self.store
            .fragments_of_box(child)
            .first()
            .and_then(|frag| self.store.fragment(*frag))
            .is_some_and(|fragment| fragment.clip == clip)
    }

    /// What a subtree the walk did not descend into reported last time.
    fn cached(&self, child: BoxKey) -> Folded {
        let fragments = self.store.fragments_of_box(child);
        let mut subtree_ink = Rect::ZERO;
        let mut blending = false;
        let mut reads = false;
        let mut disjoint = true;
        let mut rigid = true;
        let mut anchor = None;
        let mut bound = FragmentFlags::EMPTY;
        for (index, frag) in fragments.iter().enumerate() {
            let Some(fragment) = self.store.fragment(*frag) else {
                continue;
            };
            subtree_ink = if index == 0 {
                fragment.subtree_ink
            } else {
                subtree_ink.union(fragment.subtree_ink)
            };
            anchor = anchor.or(Some(fragment.anchor));
            bound = bound.union(fragment.flags.intersection(FragmentFlags::BOUND));
            blending |= fragment
                .flags
                .contains(FragmentFlags::HAS_BLENDING_DESCENDANT);
            reads |= fragment.flags.intersects(FragmentFlags::READS_BACKDROP);
            disjoint &= fragment.subtree_disjoint;
            rigid &= fragment.subtree_rigid;
        }
        // Every piece of a box spans the box's range, which counts once a piece resolved.
        let order = if anchor.is_some() {
            self.store.subtree_order(child)
        } else {
            (HitOrder::MAX, 0)
        };
        Folded {
            subtree_ink,
            blending,
            reads,
            disjoint,
            rigid,
            order,
            anchor,
            bound,
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
        between: (HitOrder, HitOrder),
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

        // Where the pieces born below take their order keys: after the last piece kept and after
        // everything painted before this box, and before the first thing its children index.
        let born = count.saturating_sub(keep) as HitOrder;
        let gap = (born > 0).then(|| {
            let mut low = between.0;
            for slot in 0..keep {
                let kind = self.scratch.kinds[kinds_mark + slot];
                if let Some(frag) = self.store.reusable_fragment(key, slot, kind)
                    && let Some(held) = self.hit.order_of(frag)
                {
                    low = low.max(held);
                }
            }
            (low, self.first_indexed_below(key, between.1))
        });

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
                    let (low, high) = gap.unwrap_or(between);
                    let index = (slot - keep) as HitOrder + 1;
                    // Spread evenly through the gap. A gap too narrow for the pieces born into
                    // it hands out a key twice, and the renumber after the walk resolves it.
                    let key_in_gap = if high.saturating_sub(low) > born {
                        low + (high - low) / (born + 1) * index
                    } else {
                        self.renumber = true;
                        low
                    };
                    self.born_order = Some(key_in_gap);
                    self.store.insert_fragment(key, |fragment| {
                        fragment.kind = kind;
                    })
                }
            };
            moved |= self.update(frag, kind, &geometry, parent, owed) != Change::Identical;
            self.born_order = None;
            self.scratch.written.push(frag);
        }
        self.scratch.kinds.truncate(kinds_mark);
        (written_mark..self.scratch.written.len(), moved)
    }

    /// The first order key indexed below `key`, or `after` when nothing below it is indexed yet.
    ///
    /// The lowest key of all the children, because a box can paint its children in another order
    /// than it lays them out.
    fn first_indexed_below(&self, key: BoxKey, after: HitOrder) -> HitOrder {
        self.store
            .node(key)
            .children
            .iter()
            .filter_map(|&child| {
                let &frag = self.store.fragments_of_box(child).first()?;
                self.store.fragment(frag)?;
                let order = self.store.subtree_order(child).0;
                (order != HitOrder::MAX).then_some(order)
            })
            .min()
            .unwrap_or(after)
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
            let Some((gone, anchor)) = self
                .store
                .fragment(frag)
                .map(|it| (it.subtree_ink, it.anchor))
            else {
                continue;
            };
            self.damage_beyond_a_move(self.device_before(gone, anchor), Admitted::everything());
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
                border: placed.border,
                padding: placed.padding,
                scrollbar: placed.scrollbar,
                ink: placed.ink,
                local_ink: placed.local_ink,
                flags: placed.flags,
                clip: placed.clip,
                transform: placed.transform,
                anchor: placed.anchor,
                extra: crate::fragment::FragmentExtra {
                    clip_transform: placed.clip_transform,
                    transform_hash: placed.transform_hash,
                    content_hash: 0,
                },
                stacking: placed.stacking,
                scroll: placed.scroll,
                reads_outside: placed.reads_outside,
            };
        }
        // A bar sits in the gutter, which is inside the box's padding box against its border and
        // outside its padding and content box, so it is placed from the box's own rectangles rather than from anything the content
        // produced — and it is drawn under the chain that clips the *scroller*, never under the one
        // the scroller imposes on what it scrolls, or it would disappear along with the content.
        if let FragmentKind::Scrollbar { axis, part } = kind {
            let rect = crate::scroll_region::bar::rect(&self.scrollport(key, placed), axis, part);
            return Geometry {
                border_box: rect,
                border: zgui_geom::Edges::ZERO,
                padding: zgui_geom::Edges::ZERO,
                scrollbar: Size::new(DevicePx(0.0), DevicePx(0.0)),
                ink: on_device(placed, rect),
                local_ink: rect,
                flags: FragmentFlags::EMPTY,
                clip: placed.clip,
                transform: placed.transform,
                anchor: placed.anchor,
                extra: crate::fragment::FragmentExtra {
                    clip_transform: placed.clip_transform,
                    transform_hash: placed.transform_hash,
                    content_hash: 0,
                },
                stacking: placed.stacking,
                scroll: placed.scroll,
                reads_outside: false,
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
        let local_ink = resolved.map_or(Rect::ZERO, |line| {
            crate::fragment::ink::of_line(
                &self.store.node(key).style,
                rect,
                line.extents,
                line.ink,
                self.tables.device.scale,
            )
        });
        let mut content_hash = resolved.map_or(0, crate::inline::ellipsis::line_hash);
        let mut local_ink = local_ink;
        let mut flags = FragmentFlags::EMPTY;
        if let Some(line_box) = resolved
            && let Some(effects) = self.inline_effects(key, placed, line, rect, line_box)
        {
            local_ink = local_ink.union(effects.ink);
            content_hash ^= effects.hash;
            flags = FragmentFlags::INLINE_EFFECTS;
        }
        Geometry {
            border_box: rect,
            border: zgui_geom::Edges::ZERO,
            padding: zgui_geom::Edges::ZERO,
            scrollbar: Size::new(DevicePx(0.0), DevicePx(0.0)),
            ink: on_device(placed, local_ink),
            local_ink,
            // A line is drawn inside its own box's clip and transform, and establishes neither.
            flags,
            clip: placed.descent.clip,
            transform: placed.transform,
            anchor: placed.anchor,
            extra: crate::fragment::FragmentExtra {
                // A line is drawn under the clip its own box imposes, which that box measured in
                // its own space — the space its own transform maps onto the device.
                clip_transform: placed.transform,
                transform_hash: placed.transform_hash,
                content_hash,
            },
            stacking: placed.stacking,
            scroll: placed.scroll,
            reads_outside: false,
        }
    }

    /// What the nested inline boxes on one line paint, or nothing when none of them paints.
    ///
    /// Asked only of a context that holds nested inline boxes, so a paragraph of plain text pays
    /// one empty-slice test per line.
    fn inline_effects(
        &self,
        key: BoxKey,
        placed: &Placed,
        line: usize,
        rect: Rect<DevicePx, Device>,
        line_box: &crate::inline::lines::LineBox,
    ) -> Option<InlineEffects> {
        let resolution = self.store.inline_resolution(key)?;
        let slices = resolution.slices_on(line);
        if slices.is_empty() {
            return None;
        }
        let root = &self.store.node(key).style;
        let scale = self.tables.device.scale;
        let mut held: Option<InlineEffects> = None;
        for slice in slices {
            let style = &self.store.node(slice.box_).style;
            if !crate::fragment::ink::inline_effects(style, root, slice) {
                continue;
            }
            let part = crate::inline::slices::device_rect(placed.content_box, slice);
            let ink = crate::fragment::ink::of_inline(
                style,
                part,
                rect,
                line_box.extents,
                line_box.ink,
                scale,
            );
            let hash = zgui_scene::ContentHash::new()
                .u32(slice.box_.index())
                .f32(part.origin.x.0 - rect.origin.x.0)
                .f32(part.origin.y.0 - rect.origin.y.0)
                .f32(part.size.width.0)
                .f32(part.size.height.0)
                .u32(u32::from(slice.first) | u32::from(slice.last) << 1)
                .f32(slice.shift)
                .finish();
            held = Some(match held {
                Some(effects) => InlineEffects {
                    ink: effects.ink.union(ink),
                    hash: effects.hash.rotate_left(5) ^ hash,
                },
                None => InlineEffects { ink, hash },
            });
        }
        held
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
            padding_box: placed.padding_box,
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
        // A box drawn in an animated background colour paints, also when its own style paints
        // nothing: the colour is beside the shared style, as an animated opacity is.
        let painting;
        let next = if kind == FragmentKind::Box
            && next.flags.contains(FragmentFlags::PAINTS_NOTHING)
            && self.dirty.animates_background(node)
        {
            painting = Geometry {
                flags: next.flags.without(FragmentFlags::PAINTS_NOTHING),
                ..*next
            };
            &painting
        } else {
            next
        };
        let previous = self.store.fragment(frag).cloned();
        // Read before the write below replaces it.
        let held = previous
            .as_ref()
            .map_or(crate::fragment::FragmentExtra::NONE, |previous| {
                self.store.fragment_extra(previous)
            });
        // A name is kept across a change of content — a line that is still line two of its box is
        // still the same line — so the content is compared here rather than at the name. Nothing
        // else would catch it: the geometry of a line whose characters changed for characters of
        // the same width is identical, and the recorded painting of the old ones would be replayed
        // straight back onto the screen.
        let change = match previous.as_ref() {
            Some(previous) if previous.kind != kind => Change::Changed,
            Some(previous) => compare(previous, &held, next),
            None => Change::Changed,
        };

        // An extra equal to the one held under the same transform packs to the same record.
        let repack = previous
            .as_ref()
            .is_none_or(|previous| held != next.extra || previous.transform != next.transform);
        let mut spill = None;
        if let Some(fragment) = self.store.fragment_mut(frag) {
            fragment.kind = kind;
            fragment.parent = parent;
            let kept = fragment.flags.intersection(FragmentFlags::FOLDED);
            fragment.border_box = next.border_box;
            fragment.border = next.border;
            fragment.padding = next.padding;
            fragment.scrollbar = next.scrollbar;
            fragment.ink = next.ink;
            fragment.local_ink = next.local_ink;
            fragment.clip = next.clip;
            fragment.transform = next.transform;
            fragment.anchor = next.anchor;
            fragment.set_stacking(next.stacking);
            fragment.set_scroll(next.scroll);
            fragment.flags = next.flags.union(kept);
            // The style's half is decided where the box is placed; an animated opacity lives in
            // the document, beside the shared style.
            if kind == FragmentKind::Box && self.dirty.animates_opacity(node) {
                fragment.flags = fragment.flags.union(FragmentFlags::GROUPS_SUBTREE);
            }
            if change != Change::Identical {
                counter::bump(Counter::FragmentsRebuilt);
            }
            // After the transform, which the record states the clip space against.
            if repack {
                let was_spilled = fragment.packed.has(crate::fragment::Packed::SPILLED);
                spill = Some((was_spilled, fragment.pack_extra(next.extra)));
            }
        }
        // The store holds what the record cannot state. The content fingerprint is kept because
        // the painter keys its recording on it: a line whose cut moved while its rectangle stood
        // still is re-encoded only if the fragment says so.
        if let Some(spill) = spill {
            self.store.settle_spill(frag, spill, next.extra);
        }
        self.store.set_read_extent(frag, next.reads_outside);

        // A line that paints its nested inline boxes repaints when one of them does: those boxes
        // are below the line's own box, so their marks are in its subtree.
        let own = if matches!(kind, FragmentKind::Line { .. })
            && (next.flags.contains(FragmentFlags::INLINE_EFFECTS)
                || previous
                    .as_ref()
                    .is_some_and(|previous| previous.flags.contains(FragmentFlags::INLINE_EFFECTS)))
        {
            owed.own | (owed.subtree & REPAINTS_IN_PLACE)
        } else {
            owed.own
        };
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
                    self.damage_beyond_a_move(self.device_now(next.ink, next.anchor), admitted);
                }
                if own.contains(Dirty::REHIT) {
                    self.touch_hit(frag, change, false);
                }
            }
            Change::TranslatedOnly => {
                self.dirty.mark(owed.node, Dirty::REPOSITION | Dirty::REHIT);
                // Carried somewhere else and nothing else about it changed, which is the claim
                // a move makes: a widget that moved owes a rectangle, and deriving its role,
                // its name and its child list again to answer that is what makes a relayout of
                // a long list cost a projection of every row.
                // Reported under the element the piece carries semantics for, not under the box's
                // own source: a run of text is owed by its text node, and a text node is no
                // accessibility node at all.
                if self.dirty.is_semantic(node) {
                    self.dirty.moved(node);
                }
                if let Some(previous) = &previous {
                    let was = self.device_before(previous.ink, previous.anchor);
                    self.damage_beyond_a_move(was, Admitted::everything());
                }
                self.damage_beyond_a_move(self.device_now(next.ink, next.anchor), admitted);
                self.touch_hit(frag, change, false);
            }
            Change::Changed => {
                // A piece whose size moved and nothing else — the note cell that widened, the
                // row that grew — owes the accessibility tree its rectangle, which is the same
                // claim a carried piece makes; deriving the node's role, name, relations and
                // child list again to answer it is what made a relayout cost a projection per
                // element. A piece that appeared, changed what it paints or whether it clips
                // keeps the whole projection.
                // A line is one of a box's pieces, not the box: it is born again whenever its
                // paragraph is shaped again, and a node whose lines were shaped again owes the
                // tree no more than a node whose box grew.
                let resized = matches!(kind, FragmentKind::Line { .. })
                    || previous.as_ref().is_some_and(|previous| {
                        previous.kind == kind && geometry::only_geometry_differs(previous, next)
                    });
                let a11y = if resized && self.dirty.is_semantic(node) {
                    self.dirty.moved(node);
                    Dirty::empty()
                } else {
                    self.a11y(node)
                };
                self.dirty
                    .mark(owed.node, Dirty::REPAINT | Dirty::REHIT | a11y);
                // A paintless box that moved rigidly, or stood still while its inner boxes
                // repositioned, owes the frame no damage of its own. Repaint marks do not hold
                // it back: a repaint of nothing is nothing, and a style that *starts* painting
                // flips [`FragmentFlags::PAINTS_NOTHING`], which the comparison reads on both
                // sides.
                let repositioned = kind == FragmentKind::Box
                    && previous
                        .as_ref()
                        .is_some_and(|previous| repositioned_within(previous, &held, next));
                // A box born painting nothing owes nothing of its own: whatever is drawn inside it
                // belongs to pieces born with it, which damage themselves, and what stood there
                // before is damaged by the pieces that left.
                let born_empty =
                    self.born_order.is_some() && paints_nothing_itself(kind, next.flags);
                if !repositioned && !born_empty {
                    match previous
                        .as_ref()
                        .and_then(|previous| self.resized_edges(previous, &held, kind, next))
                    {
                        // Only the strips along the edges that moved paint anything different.
                        Some(strips) => {
                            for strip in strips.into_iter().flatten() {
                                self.damage_beyond_a_move(strip, Admitted::everything());
                            }
                        }
                        None => {
                            if let Some(previous) = &previous {
                                let was = self.device_before(previous.ink, previous.anchor);
                                self.damage_beyond_a_move(was, Admitted::everything());
                            }
                            let is = self.device_now(next.ink, next.anchor);
                            self.damage_beyond_a_move(is, admitted);
                        }
                    }
                }
                // Whether the piece's coordinate space is where it was: only then can an entry
                // that compares equal be trusted to mean nothing moved under the pointer.
                let stood = previous.as_ref().is_some_and(|previous| {
                    previous.transform == next.transform
                        && held.transform_hash == next.extra.transform_hash
                        && held.clip_transform == next.extra.clip_transform
                });
                self.touch_hit(frag, change, stood);
            }
        }
        // An opacity, filter or blend mode applied to the subtree changes every pixel the subtree
        // paints. The box's own ink does not hold the pixels its subtree paints outside it, which
        // are all of them when the box paints nothing. A box that moved also owes the subtree at
        // its new place: the pixels a scroll shifts there carry the old effect. Cut to the chain,
        // which keeps a translucent pane as tall as its list to the part of it on the screen.
        // Any repaint mark fires this, also one a structural edit below the box puts on it.
        if kind == FragmentKind::Box
            && own.contains(Dirty::REPAINT)
            && let Some(previous) = &previous
        {
            let groups = self
                .store
                .fragment(frag)
                .is_some_and(|fragment| fragment.flags.contains(FragmentFlags::GROUPS_SUBTREE));
            if groups || previous.flags.contains(FragmentFlags::GROUPS_SUBTREE) {
                let was = self.device_before(previous.subtree_ink, previous.anchor);
                self.damage_beyond_a_move(was, admitted);
                let shift = next.border_box.origin - previous.border_box.origin;
                if shift != zgui_geom::Point::new(DevicePx(0.0), DevicePx(0.0)) {
                    let carried = previous.subtree_ink.translate(Size::new(shift.x, shift.y));
                    let carried = self.device_now(carried, next.anchor);
                    self.damage_beyond_a_move(carried, admitted);
                }
            }
        }
        change
    }

    /// The strips a box that changed only its size owes, when its painting changes only near the
    /// edges that moved.
    ///
    /// `None` sends the box down the ordinary path, where its whole ink before and after is owed.
    /// That is every case whose painting depends on its size elsewhere (see
    /// [`ink::edge_reach`](crate::fragment::ink::edge_reach)), and every case where the device
    /// rectangle is not simply the border box moved: a decoration reaching outside it, a matrix of
    /// its own or a different one, a border of a different width.
    fn resized_edges(
        &self,
        previous: &crate::fragment::Fragment,
        held: &crate::fragment::FragmentExtra,
        kind: FragmentKind,
        next: &Geometry,
    ) -> Option<[Option<Rect<DevicePx, Device>>; 4]> {
        let unflagged = |flags: FragmentFlags| flags.without(FragmentFlags::UNCOMPARED);
        if kind != FragmentKind::Box
            || previous.kind != kind
            || unflagged(previous.flags) != unflagged(next.flags)
            || next.flags.contains(FragmentFlags::HAS_TRANSFORM)
            || next.flags.contains(FragmentFlags::HAS_READ_EXTENT)
            || previous.border != next.border
            || previous.transform != next.transform
            || previous.anchor != next.anchor
            || held.transform_hash != next.extra.transform_hash
            || held.clip_transform != next.extra.clip_transform
            || held.content_hash != next.extra.content_hash
            || previous.ink.size != previous.border_box.size
            || next.ink.size != next.border_box.size
        {
            return None;
        }
        let node = self.store.get(previous.box_)?;
        let reach = crate::fragment::ink::edge_reach(
            &node.style,
            self.store.custom_properties(previous.box_, &node.style),
            node.painted,
            previous.border_box,
            next.border_box,
            next.border,
            self.tables.device.scale,
        )?;
        Some(edge_strips(
            self.device_before(previous.ink, previous.anchor),
            self.device_now(next.ink, next.anchor),
            reach,
        ))
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
    ///
    /// `stood` says the piece's coordinate space did not move, which is what lets an entry that
    /// compares equal to the held one be left alone — see
    /// [`HitIndex::refresh`](crate::fragment::hit::HitIndex::refresh).
    fn touch_hit(&mut self, frag: FragKey, change: Change, stood: bool) {
        let scale = self.tables.device.scale;
        let held = self.hit.order_of(frag).or(self.born_order).unwrap_or(0);
        if let Some(entry) = crate::fragment::hit::entry_for(self.store, frag, held, scale) {
            match change {
                Change::TranslatedOnly => self.hit.translate(frag, entry),
                Change::Changed if stood => self.hit.refresh(frag, entry),
                Change::Identical | Change::Changed => self.hit.update(frag, entry),
            }
        }
    }

    /// Writes the unwind answers onto this box's fragments.
    fn record_fold(
        &mut self,
        fragments: core::ops::Range<usize>,
        subtree_ink: Rect<DevicePx, Device>,
        folded: FragmentFlags,
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
            fragment.flags = fragment.flags.without(FragmentFlags::FOLDED).union(folded);
        }
    }
}

/// What the nested inline boxes on one line paint.
struct InlineEffects {
    /// The union of what they paint, in the line's own space.
    ink: Rect<DevicePx, Device>,
    /// A fingerprint of where their parts are, relative to the line box.
    hash: u64,
}

/// Whether a piece is a box whose own painting is nothing.
fn paints_nothing_itself(kind: FragmentKind, flags: FragmentFlags) -> bool {
    kind == FragmentKind::Box && flags.contains(FragmentFlags::PAINTS_NOTHING)
}

/// The strips along every edge that differs between `was` and `is`, each reaching `reach` to
/// either side of both positions of its edge and spanning both rectangles across it.
///
/// Together they cover everything one rectangle holds and the other does not, and the band inside
/// each moved edge where a border or a corner is drawn.
fn edge_strips(
    was: Rect<DevicePx, Device>,
    is: Rect<DevicePx, Device>,
    reach: f32,
) -> [Option<Rect<DevicePx, Device>>; 4] {
    let span = was.union(is);
    let across = |from: f32, to: f32| (from.min(to) - reach, from.max(to) + reach);
    let vertical = |from: f32, to: f32| {
        (from != to).then(|| {
            let (left, right) = across(from, to);
            Rect::new(
                zgui_geom::Point::new(DevicePx(left), span.origin.y),
                Size::new(DevicePx(right - left), span.size.height),
            )
        })
    };
    let horizontal = |from: f32, to: f32| {
        (from != to).then(|| {
            let (top, bottom) = across(from, to);
            Rect::new(
                zgui_geom::Point::new(span.origin.x, DevicePx(top)),
                Size::new(span.size.width, DevicePx(bottom - top)),
            )
        })
    };
    [
        vertical(was.left().0, is.left().0),
        vertical(was.right().0, is.right().0),
        horizontal(was.top().0, is.top().0),
        horizontal(was.bottom().0, is.bottom().0),
    ]
}

#[cfg(test)]
mod edge_tests {
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};

    use super::edge_strips;

    fn at(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(x), DevicePx(y)),
            Size::new(DevicePx(width), DevicePx(height)),
        )
    }

    #[test]
    fn a_box_whose_bottom_edge_moved_owes_one_strip_around_that_edge() {
        let strips = edge_strips(at(0.0, 0.0, 100.0, 50.0), at(0.0, 0.0, 100.0, 60.0), 2.0);
        let owed: Vec<_> = strips.into_iter().flatten().collect();
        assert_eq!(owed, [at(0.0, 48.0, 100.0, 14.0)]);
    }

    #[test]
    fn the_strips_cover_everything_either_rectangle_holds_alone() {
        let was = at(10.0, 20.0, 100.0, 50.0);
        let is = at(5.0, 30.0, 140.0, 30.0);
        let strips: Vec<_> = edge_strips(was, is, 1.0).into_iter().flatten().collect();
        assert_eq!(strips.len(), 4);
        for x in (0..160).map(|x| x as f32 + 0.5) {
            for y in (0..80).map(|y| y as f32 + 0.5) {
                let point = Point::new(DevicePx(x), DevicePx(y));
                if was.contains(point) != is.contains(point) {
                    assert!(
                        strips.iter().any(|strip| strip.contains(point)),
                        "({x}, {y}) is in one rectangle only and in no strip"
                    );
                }
            }
        }
    }
}
