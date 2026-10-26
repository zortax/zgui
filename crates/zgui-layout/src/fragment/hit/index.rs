//! The index that answers what is under a point.

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use zgui_arena::SlotVec;
use zgui_dom::NodeKey;
use zgui_dom::side::BoxKey;
use zgui_geom::{Corners, Device, DevicePx, Point, Rect, Size, Vec2};
use zgui_profile::{Counter, counter};
use zgui_scene::{ClipId, ClipLink, ClipNode, ClipTable, SpatialId, SpatialTree};

use crate::fragment::FragKey;
use crate::fragment::hit::HitOrder;
use crate::fragment::hit::entry::{HitEntry, covers_rounded};
use crate::fragment::hit::pointer_events::PointerEvents;
use crate::fragment::hit::rtree::{Carried, Forest, Placed};
use crate::fragment::hit::transform;
use crate::tree::store::LayoutStore;

/// How much of the index may be touched one entry at a time before rebuilding it is cheaper.
///
/// A quarter is the point where the hierarchy has been reshaped enough that a fresh build produces
/// a better one than the incremental inserts did. Below it, rebuilding costs the whole document to
/// service work proportional to what moved.
pub const CHURN_FRACTION: usize = 4;

/// The distance a bulk build leaves between consecutive order keys.
///
/// Room for a million fragments to be born between two neighbours before the keys have to be
/// handed out again, which a single frame never needs and a long-lived document reaches only
/// through the renumber the fragment pass asks for when a gap closes.
pub const ORDER_STRIDE: HitOrder = 1 << 20;

/// Hit-test index.
///
/// Entries have identity: one per fragment, updated in place. The index *carries* the painting
/// order rather than assigning one, so — unlike the structure that assigns draw order — its
/// contents do not depend on the sequence entries were added in, and one entry can be moved without
/// touching any other. That is what makes the incremental path sound as well as cheap: a transform
/// transition marks a subtree on every tick, and rebuilding the whole index for each of those would
/// cost the document per frame.
#[derive(Debug, Default)]
pub struct HitIndex {
    /// One entry per fragment, less its corner radii.
    entries: SlotVec<FragKey, Stored>,
    /// The corner radii of each entry with rounded corners, keyed by the entry's own name.
    radii: FxHashMap<FragKey, Corners<Vec2<DevicePx>>>,
    /// One spatial hierarchy per coordinate system, over the envelopes of the entries in it.
    forest: Forest,
    /// Entries touched since the last bulk build.
    churn: u32,
    /// Whether a bulk rebuild has become owed, latched until it runs.
    ///
    /// Latched rather than re-derived, because the threshold compares churn against how many
    /// entries there are, and a pass that removes many entries and then inserts more can carry
    /// the comparison back under the line — while the hierarchy work skipped in between is only
    /// sound if the rebuild actually happens.
    owed_rebuild: bool,
    /// The entries written by [`HitIndex::carry`] that the hierarchy has not been told about yet.
    ///
    /// Held on the index rather than raised at each call site so that the buffer outlives the frame
    /// and a scroll allocates nothing for it.
    carried: Vec<Carried>,
    /// Moved by every write, removal and rebuild.
    ///
    /// What is under a stationary pointer can change only when an entry did, so a reader that
    /// remembers the generation it last answered against knows whether to ask again.
    generation: u64,
}

impl HitIndex {
    /// An empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many fragments are indexed.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is indexed.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// A number that moves with every change to the index, and never otherwise.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Records that a coordinate system some entries are filed in moved, though no entry did.
    ///
    /// What is under a stationary pointer then changed, so the generation moves.
    pub fn note_space_moved(&mut self) {
        self.generation += 1;
    }

    /// How many entries have been touched since the last bulk build.
    pub fn churn(&self) -> u32 {
        self.churn
    }

    /// Whether the spatial hierarchy holds exactly the entries the index does.
    ///
    /// The two are written together and can only disagree through a bug — an entry moved without
    /// its old rectangle being taken out leaves a name in the hierarchy that answers hits it should
    /// not. That is silent everywhere except here.
    pub fn is_consistent(&self) -> bool {
        self.forest.len() == self.entries.len()
    }

    /// How many coordinate systems anything is indexed in.
    ///
    /// One hierarchy each, so this is also how many descents one query costs — the point is carried
    /// into every one of them before its tree is asked anything.
    pub fn spaces(&self) -> usize {
        self.forest.spaces()
    }

    /// What the index holds for one fragment.
    pub fn entry(&self, frag: FragKey) -> Option<HitEntry> {
        self.entries.get(frag).map(|stored| self.unpack(stored))
    }

    /// How many entries keep corner radii beside the entry.
    #[cfg(test)]
    fn rounded(&self) -> usize {
        self.radii.len()
    }

    /// The whole entry that one stored entry stands for.
    fn unpack(&self, stored: &Stored) -> HitEntry {
        HitEntry {
            frag: stored.frag,
            node: stored.node,
            order: stored.order,
            clip: stored.clip,
            space: stored.space,
            pointer_events: stored.pointer_events,
            radii: self.radii_of(stored),
            bounds: stored.bounds,
            envelope: stored.envelope,
        }
    }

    /// The corner radii of one stored entry.
    fn radii_of(&self, stored: &Stored) -> Corners<Vec2<DevicePx>> {
        if !stored.rounded {
            return SQUARE;
        }
        self.radii.get(&stored.frag).copied().unwrap_or(SQUARE)
    }

    /// Stores one entry, keeping its corner radii beside it only when they are not zero.
    fn pack(&mut self, frag: FragKey, entry: HitEntry) {
        let stored = Stored::of(&entry);
        if let Some(held) = self.entries.insert(frag, stored)
            && held.rounded
            && (!stored.rounded || held.frag != stored.frag)
        {
            self.radii.remove(&held.frag);
        }
        if stored.rounded {
            self.radii.insert(stored.frag, entry.radii);
        }
    }

    /// Whether `point`, already in the entry's own space, lands on the stored entry.
    ///
    /// The same answer as [`HitEntry::covers`].
    fn covers(&self, stored: &Stored, point: Point<DevicePx, Device>) -> bool {
        if !stored.rounded {
            return stored.bounds.contains(point);
        }
        covers_rounded(stored.bounds, self.radii_of(stored), point)
    }

    /// Every fragment the index holds an entry for, in no particular order.
    pub fn indexed(&self) -> impl Iterator<Item = FragKey> + '_ {
        self.entries.iter().map(|(_, stored)| stored.frag)
    }

    /// Moves one fragment whose shape may have changed as well as its position.
    ///
    /// One call per fragment whose geometry, clip, transform or hit-relevant style changed, and
    /// independent of every other entry.
    ///
    /// # What counts as churn
    ///
    /// Churn estimates how far the spatial hierarchy has drifted from one built in a single pass.
    /// Two things have to be true of a write before it has drifted anything, and both are tested
    /// here rather than assumed. The caller has to be reporting a change of *shape*, because a
    /// change of position alone leaves every entry with the neighbours it had — see
    /// [`translate`](HitIndex::translate). And the hierarchy has to have actually moved the entry
    /// between nodes, which most writes do not: an entry rewritten inside the leaf already holding
    /// it has left that hierarchy exactly as it found it, so counting the write would rebuild the
    /// whole index every few frames for as long as anything on screen kept changing colour.
    pub fn update(&mut self, frag: FragKey, entry: HitEntry) {
        if self.write(frag, entry) == Placed::Reinserted {
            self.churn = self.churn.saturating_add(1);
        }
    }

    /// Rewrites one fragment whose coordinate space stood still, when its entry changed at all.
    ///
    /// For a fragment the pass found changed in a way the index does not record — a paint flag,
    /// a cut line — under a transform that did not move. An entry rewritten as exactly what it
    /// was changes nothing under any point: the hierarchy already holds it where it is, and no
    /// reader of the generation has anything new to ask. A fragment whose space moved must go
    /// through [`update`](HitIndex::update) even when its entry compares equal, because the
    /// entry is in that space and says nothing about where the space went.
    pub fn refresh(&mut self, frag: FragKey, entry: HitEntry) {
        if let Some(held) = self.entries.get(frag)
            && *held == Stored::of(&entry)
            && self.radii_of(held) == entry.radii
        {
            return;
        }
        self.update(frag, entry);
    }

    /// Moves one fragment that has changed position and nothing else.
    ///
    /// The same write, and never counted as churn. Every entry of a scrolled container or a
    /// transformed subtree moves by the same vector, keeps its extent, and keeps its relationship
    /// to its neighbours, so the hierarchy above them is as good afterwards as before. Counting
    /// these would make the two commonest whole-subtree movements there are — a scroll and a
    /// transform transition — rebuild the entire document's index every few frames, which is the
    /// exact cost the incremental path exists to avoid.
    pub fn translate(&mut self, frag: FragKey, entry: HitEntry) {
        self.write(frag, entry);
    }

    /// Moves one fragment that is part of a run moving together, leaving the hierarchy for later.
    ///
    /// The entry itself is written straight away, so anything that reads an entry by name reads the
    /// new one. What is deferred is the hierarchy *above* the entries, and it is deferred because
    /// answering it one entry at a time asks each of them to fit inside a leaf drawn around where
    /// its neighbours used to be, which stretches that leaf across the gap or searches the entry
    /// back into the hierarchy it never really left.
    ///
    /// A run is ended by [`HitIndex::settle`], and until it is, the hierarchy answers for the
    /// rectangles the run's entries had *before* it. Every caller of this therefore owes a settle
    /// before anything queries the index, which for the fragment pass is the same call that ends
    /// the walk.
    pub fn carry(&mut self, frag: FragKey, entry: HitEntry) {
        self.generation += 1;
        self.carried.push(Carried {
            frag,
            space: entry.space,
            bounds: entry.envelope,
        });
        self.pack(frag, entry);
        counter::bump(Counter::HitEntriesUpdated);
    }

    /// Moves the entry held for one fragment by `by` and puts it under `clip`, as part of a run.
    ///
    /// The same write as [`HitIndex::carry`] for an entry that keeps everything except its two
    /// rectangles and its chain. The entry changes in place. Returns `false` when the index holds
    /// no entry for `frag`.
    pub fn carry_by(&mut self, frag: FragKey, by: Size<DevicePx, Device>, clip: ClipId) -> bool {
        let Some(stored) = self.entries.get_mut(frag) else {
            return false;
        };
        stored.bounds = stored.bounds.translate(by);
        stored.envelope = stored.envelope.translate(by);
        stored.clip = clip;
        self.carried.push(Carried {
            frag,
            space: stored.space,
            bounds: stored.envelope,
        });
        self.generation += 1;
        counter::bump(Counter::HitEntriesUpdated);
        true
    }

    /// Ends a run of [`HitIndex::carry`], bringing the hierarchy up to date in one pass.
    ///
    /// Costs nothing when no run was started, so a pass that moved nothing rigidly may call it
    /// unconditionally.
    pub fn settle(&mut self) {
        if self.carried.is_empty() {
            return;
        }
        let carried = core::mem::take(&mut self.carried);
        self.forest.settle(&carried);
        // Handed back rather than dropped: the buffer is the whole reason a scroll allocates
        // nothing here, and `settle` is the only thing that empties it.
        self.carried = carried;
        self.carried.clear();
    }

    /// The write both paths share, reporting what the hierarchy had to do to take it.
    ///
    /// Once enough has churned that the pass ends in a bulk rebuild anyway, the hierarchy is dead
    /// weight: the rebuild derives it from the store whole, so every placement between here and
    /// there is thrown-away work. The entry itself is still written — a read by name must see the
    /// new one — and the hierarchy is left stale, which is sound because a crossed threshold can
    /// never uncross before the rebuild resets it. A resize frame touches every fragment, and this
    /// is what keeps it from paying one tree surgery each on the way to a rebuild it already owes.
    fn write(&mut self, frag: FragKey, entry: HitEntry) -> Placed {
        self.generation += 1;
        if self.owes_rebuild() {
            self.pack(frag, entry);
            counter::bump(Counter::HitEntriesUpdated);
            return Placed::InPlace;
        }
        let placed = self.forest.place(frag, entry.space, entry.envelope);
        self.pack(frag, entry);
        counter::bump(Counter::HitEntriesUpdated);
        placed
    }

    /// Takes one fragment out.
    pub fn remove(&mut self, frag: FragKey) {
        if let Some(stored) = self.entries.remove(frag) {
            if stored.rounded {
                self.radii.remove(&stored.frag);
            }
            self.generation += 1;
            self.churn = self.churn.saturating_add(1);
            if !self.owes_rebuild() {
                self.forest.remove(frag);
            }
        }
    }

    /// Whether the index has been touched enough that rebuilding it is the cheaper option.
    pub fn should_rebuild(&self) -> bool {
        self.owed_rebuild
            || (!self.entries.is_empty()
                && self.churn as usize > self.entries.len() / CHURN_FRACTION)
    }

    /// The same question, latching a yes until the rebuild it obliges has run.
    fn owes_rebuild(&mut self) -> bool {
        self.owed_rebuild = self.should_rebuild();
        self.owed_rebuild
    }

    /// Builds the whole index again from the fragment tree.
    ///
    /// For when painting order itself moved — a `z-index` change restacks the document and every
    /// entry's order may be different — or when so much has been updated one entry at a time that
    /// the hierarchy is no longer a good one.
    pub fn rebuild(&mut self, store: &mut LayoutStore, scale: f32) {
        self.generation += 1;
        self.entries.clear();
        self.radii.clear();
        self.forest.clear();
        self.churn = 0;
        self.owed_rebuild = false;
        // A run that had not been settled named entries this build is about to replace, and
        // settling it afterwards would file rectangles for fragments the hierarchy has just been
        // told about properly.
        self.carried.clear();
        counter::bump(Counter::HitIndexRebuilds);
        let Some(root) = store.root() else {
            return;
        };
        let mut order = ORDER_STRIDE;
        for box_ in crate::fragment::stacking::paint_order(store, root) {
            for &frag in store.fragments_of_box(box_) {
                let Some(entry) = crate::fragment::hit::entry_for(store, frag, order, scale) else {
                    continue;
                };
                self.forest.insert(frag, entry.space, entry.envelope);
                self.pack(frag, entry);
                order += ORDER_STRIDE;
            }
        }
        self.fold_orders(store, root);
    }

    /// The order key one fragment holds, if it is indexed.
    pub fn order_of(&self, frag: FragKey) -> Option<HitOrder> {
        self.entries.get(frag).map(|entry| entry.order)
    }

    /// Writes onto every box with fragments the range of order keys its subtree spans.
    ///
    /// The range is what the fragment pass places a newborn fragment between: a subtree's keys
    /// are contiguous in painting order, so what comes before a box is the end of the range of
    /// whatever is painted before it. Minimum and maximum are the same whatever order the
    /// children are visited in, so this walks the layout list and asks nothing about painting.
    fn fold_orders(&self, store: &mut LayoutStore, key: BoxKey) -> (HitOrder, HitOrder) {
        let mut range = (HitOrder::MAX, 0);
        for &frag in store.fragments_of_box(key) {
            if let Some(entry) = self.entries.get(frag) {
                range = (range.0.min(entry.order), range.1.max(entry.order));
            }
        }
        let mut position = 0;
        while let Some(&child) = store.node(key).children.get(position) {
            let below = self.fold_orders(store, child);
            range = (range.0.min(below.0), range.1.max(below.1));
            position += 1;
        }
        if !store.fragments_of_box(key).is_empty() {
            store.set_subtree_order(key, range);
        }
        range
    }

    /// The fragments under `point`, topmost first.
    ///
    /// Everything that decides whether a fragment is under the point happens inside this call: the
    /// point is carried down into each coordinate system, tested against the entries there, tested
    /// against their clip chains and their rounded corners, and dropped where the fragment takes no
    /// pointer events.
    ///
    /// The answer is fragment names, not elements. Turning them into the ancestor chain that event
    /// dispatch walks is a question about elements, which this crate does not answer — a hit on a
    /// cell inside a row whose element generates no box of its own still has to dispatch through
    /// that row.
    ///
    /// # Which matrix, when one is moving
    ///
    /// The one the frame in front of the pointer was drawn with. A transition ticks by rewriting
    /// the matrix under a coordinate system's name; composition then draws the frame through
    /// whatever that name resolves to at that moment, and this call resolves the same name the same
    /// way. So an element half-way through moving is hit where it is seen, never where it started
    /// or where it is going — and the entries themselves are not touched by the tick at all, which
    /// is what makes that true of a frame nothing walked.
    ///
    /// ```
    /// use zgui_geom::{Device, DevicePx, Matrix4, Point, Rect, Size};
    /// use zgui_layout::fragment::hit::{HitEntry, HitIndex};
    /// use zgui_layout::fragment::transform::transformed_bounds;
    /// use zgui_scene::{ClipTable, OwnSpace, PropertyOwner, SpatialTree};
    /// # use zgui_arena::{ArenaKind, DocumentId, DomainId, Generation, Key};
    /// # let frag = Key::new(
    /// #     1,
    /// #     Generation::FIRST,
    /// #     DomainId::new(DocumentId::FIRST, ArenaKind::new(2).expect("a valid arena")),
    /// # );
    /// let clips = ClipTable::rooted();
    /// let mut spatial = SpatialTree::with_viewport();
    /// let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
    ///
    /// // A card, indexed once, in the space its own transform establishes.
    /// let square = Rect::new(
    ///     Point::new(DevicePx(0.0), DevicePx(0.0)),
    ///     Size::new(DevicePx(20.0), DevicePx(20.0)),
    /// );
    /// let mut entry = HitEntry::new(frag, square);
    /// let mut index = HitIndex::new();
    ///
    /// // Two ticks of a transition, each sampling a matrix and composing a frame through it.
    /// for x in [30.0, 60.0] {
    ///     let sampled = OwnSpace::of(Some(Matrix4::translation(x, 0.0, 0.0)), None, false);
    ///     entry.space = Some(spatial.space_of(spatial.viewport(), owner, sampled));
    ///     if x == 30.0 {
    ///         index.update(frag, entry);
    ///     }
    ///
    ///     // Where the frame drew it: the entry's own rectangle through the matrix the name
    ///     // resolves to, which is what the paint stage lowers a primitive through.
    ///     let matrix = spatial.resolve(entry.space.expect("a space")).expect("a live space");
    ///     let drawn = transformed_bounds(&matrix, entry.bounds);
    ///     assert_eq!(drawn.origin.x.0, x);
    ///
    ///     let middle = Point::new(
    ///         DevicePx(drawn.origin.x.0 + 10.0),
    ///         DevicePx(drawn.origin.y.0 + 10.0),
    ///     );
    ///     assert_eq!(
    ///         index.hit(middle, &clips, &spatial).as_slice(),
    ///         &[frag],
    ///         "the hit answers over the rectangle this frame was drawn with",
    ///     );
    ///     let before = Point::new(DevicePx(10.0), DevicePx(10.0));
    ///     assert!(
    ///         index.hit(before, &clips, &spatial).is_empty(),
    ///         "and never where the element was before the transition began",
    ///     );
    /// }
    /// ```
    pub fn hit(
        &self,
        point: Point<DevicePx, Device>,
        clips: &ClipTable,
        spatial: &SpatialTree,
    ) -> SmallVec<[FragKey; 8]> {
        // On the stack: a query holds the candidates of one coordinate system at a time, a document
        // puts a handful under any one point, and a heap allocation per pointer move is the whole
        // of what a query costs beyond the descents it is made of.
        let mut candidates: SmallVec<[FragKey; 16]> = SmallVec::new();
        let mut hits: SmallVec<[(HitOrder, FragKey); 8]> = SmallVec::new();
        let mut others = Others::at(point);
        for (space, tree) in self.forest.trees() {
            // Once per coordinate system rather than once per candidate, and a system that
            // collapses the plane — a `scale(0)`, a rotation seen exactly edge-on — dismisses
            // everything in it here instead of one entry at a time.
            let Some(local) = transform::into_local(point, space, spatial) else {
                continue;
            };
            candidates.clear();
            tree.query(local, &mut candidates);
            for &frag in &candidates {
                let Some(entry) = self.entries.get(frag) else {
                    continue;
                };
                if !entry.pointer_events.is_hittable() {
                    continue;
                }
                if !self.covers(entry, local) {
                    continue;
                }
                // Each link of the chain is tested in the space it was measured in: the clip
                // belongs to whichever ancestor imposed it, and a link tested in another space is
                // a clip applied in the wrong place on the screen.
                if !clipped_in(clips, entry.clip, (space, local), spatial, &mut others) {
                    continue;
                }
                hits.push((entry.order, frag));
            }
        }
        // Topmost first: the last thing painted is the first thing hit. The fragment name breaks a
        // tie, because the trees are visited in no fixed order and an answer that depended on which
        // coordinate system was reached first would differ between two runs of one document.
        hits.sort_unstable_by_key(|(order, frag)| (core::cmp::Reverse(*order), *frag));
        hits.into_iter().map(|(_, frag)| frag).collect()
    }
}

/// One entry as the index stores it: a [`HitEntry`] less its corner radii.
///
/// Nearly every entry is square, so the corner radii are kept beside the entry only when they are
/// not zero, and a pointer move reads them only for a candidate that has them. Radii do not change
/// when an entry moves, so a rigid move never touches them.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Stored {
    /// See [`HitEntry::frag`].
    frag: FragKey,
    /// See [`HitEntry::node`].
    node: Option<NodeKey>,
    /// See [`HitEntry::order`].
    order: HitOrder,
    /// See [`HitEntry::space`].
    space: Option<SpatialId>,
    /// See [`HitEntry::clip`].
    clip: ClipId,
    /// See [`HitEntry::pointer_events`].
    pointer_events: PointerEvents,
    /// Whether [`HitIndex::radii`] holds the entry's corner radii. Clear, the radii are zero.
    rounded: bool,
    /// See [`HitEntry::bounds`].
    bounds: Rect<DevicePx, Device>,
    /// See [`HitEntry::envelope`].
    envelope: Rect<DevicePx, Device>,
}

impl Stored {
    /// The stored form of one entry.
    fn of(entry: &HitEntry) -> Self {
        Self {
            frag: entry.frag,
            node: entry.node,
            order: entry.order,
            space: entry.space,
            clip: entry.clip,
            pointer_events: entry.pointer_events,
            rounded: entry.radii != SQUARE,
            bounds: entry.bounds,
            envelope: entry.envelope,
        }
    }
}

/// The radii of a square entry.
const SQUARE: Corners<Vec2<DevicePx>> = Corners::uniform(Vec2::splat(DevicePx(0.0)));

/// A point on the device, or in one coordinate system.
type Spot = Point<DevicePx, Device>;

/// The query point, as already carried into the coordinate systems a query's clip links named
/// apart from the hierarchy being queried.
///
/// One query meets few such systems and asks about each of them many times: every candidate under
/// a transformed box tests the links of the clipping ancestors outside it. A lookup here is cheaper
/// than a walk of the spatial chain out to the root for each one.
struct Others {
    /// The point on the device.
    point: Spot,
    /// Each system carried into so far, and the point in it; nothing where the system collapses
    /// the plane or no longer resolves.
    held: SmallVec<[(SpatialId, Option<Spot>); 4]>,
}

impl Others {
    /// A query at `point` that has carried it nowhere yet.
    fn at(point: Spot) -> Self {
        Self {
            point,
            held: SmallVec::new(),
        }
    }

    /// The point in `space`, carried there on first use.
    fn carried(&mut self, space: SpatialId, spatial: &SpatialTree) -> Option<Spot> {
        if let Some((_, local)) = self.held.iter().find(|(held, _)| *held == space) {
            return *local;
        }
        let local = transform::into_local(self.point, Some(space), spatial);
        self.held.push((space, local));
        local
    }
}

/// Whether a device point survives every link of a clip chain.
///
/// Each link is tested in its own coordinate system. A chain crosses a system boundary wherever a
/// transformed box sits between two clipping ancestors, and the links on either side of it measure
/// their rectangles in different coordinates. `current` is the hierarchy being queried and the
/// point in it, which most links name. A link whose system collapses the plane or no longer
/// resolves admits nothing; paint places such a link where it was interned instead.
fn clipped_in(
    clips: &ClipTable,
    clip: ClipId,
    current: (Option<SpatialId>, Spot),
    spatial: &SpatialTree,
    others: &mut Others,
) -> bool {
    let mut carried = |space: SpatialId| {
        if current.0 == Some(space) {
            Some(current.1)
        } else {
            others.carried(space, spatial)
        }
    };
    // Inner links first. Intersection does not depend on the order, and the innermost link is
    // usually the tightest one, so a miss is usually found soonest.
    let mut cursor = clip;
    while let Some(ClipNode::Link { link, parent, .. }) = clips.get(cursor) {
        let inside = match *link {
            ClipLink::RoundedRect {
                rect, radii, space, ..
            } => carried(space).is_some_and(|local| {
                crate::fragment::hit::entry::covers_rounded(rect, radii, local)
            }),
            // A sampled mask decides coverage per pixel, which is a question for whatever holds the
            // coverage tile. Its rectangle is the most this index can test.
            ClipLink::Mask {
                tile, transform, ..
            } => carried(transform).is_some_and(|local| {
                let bounds = tile.bounds;
                local.x.0 >= bounds.origin.x as f32
                    && local.y.0 >= bounds.origin.y as f32
                    && local.x.0 <= (bounds.origin.x + bounds.size.width) as f32
                    && local.y.0 <= (bounds.origin.y + bounds.size.height) as f32
            }),
        };
        if !inside {
            return false;
        }
        cursor = *parent;
    }
    true
}

#[cfg(test)]
mod tests {
    use zgui_arena::{ArenaKind, DocumentId, DomainId, Generation, Key};
    use zgui_geom::{Corners, DevicePx, Point, Rect, Size, Vec2};
    use zgui_scene::{ClipTable, SpatialTree};

    use super::{HitIndex, Stored};
    use crate::fragment::FragKey;
    use crate::fragment::hit::HitEntry;

    /// A fragment name.
    fn frag(index: u32) -> FragKey {
        Key::new(
            index,
            Generation::FIRST,
            DomainId::new(DocumentId::FIRST, ArenaKind::new(2).expect("a valid arena")),
        )
    }

    /// A square from the origin.
    fn square(side: f32) -> Rect<DevicePx, zgui_geom::Device> {
        Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(side), DevicePx(side)),
        )
    }

    #[test]
    fn a_stored_entry_stays_well_under_a_whole_entry() {
        let size = size_of::<Option<Stored>>();
        assert!(size <= 72, "a stored hit entry grew to {size} bytes");
    }

    #[test]
    fn an_entry_reads_back_as_it_was_written_whatever_its_shape() {
        let mut index = HitIndex::new();
        let plain = HitEntry::new(frag(1), square(20.0));
        let mut rounded = HitEntry::new(frag(2), square(20.0));
        rounded.radii = Corners::uniform(Vec2::splat(DevicePx(8.0)));
        let mut shadowed = HitEntry::new(frag(3), square(20.0));
        shadowed.envelope = square(30.0);
        for entry in [plain, rounded, shadowed] {
            index.update(entry.frag, entry);
        }
        assert_eq!(index.entry(frag(1)), Some(plain));
        assert_eq!(index.entry(frag(2)), Some(rounded));
        assert_eq!(index.entry(frag(3)), Some(shadowed));
        assert_eq!(index.rounded(), 1, "only the rounded entry keeps radii");

        index.update(frag(2), HitEntry::new(frag(2), square(20.0)));
        assert_eq!(
            index.rounded(),
            0,
            "a corner that went square drops its radii"
        );
        index.update(rounded.frag, rounded);
        index.remove(frag(2));
        assert_eq!(index.rounded(), 0, "a removed entry drops its radii");
    }

    #[test]
    fn a_carried_entry_moves_and_keeps_its_radii() {
        let mut index = HitIndex::new();
        let mut rounded = HitEntry::new(frag(1), square(20.0));
        rounded.radii = Corners::uniform(Vec2::splat(DevicePx(8.0)));
        rounded.envelope = square(30.0);
        index.update(rounded.frag, rounded);
        let by = Size::new(DevicePx(5.0), DevicePx(-3.0));
        assert!(index.carry_by(frag(1), by, rounded.clip));
        index.settle();
        let mut moved = rounded;
        moved.bounds = rounded.bounds.translate(by);
        moved.envelope = rounded.envelope.translate(by);
        assert_eq!(index.entry(frag(1)), Some(moved));
        assert_eq!(index.rounded(), 1);
        assert!(index.is_consistent());
        assert!(
            !index.carry_by(frag(2), by, rounded.clip),
            "an entry the index does not hold is not carried"
        );
    }

    #[test]
    fn a_rounded_corner_is_missed_through_the_index() {
        let clips = ClipTable::rooted();
        let spatial = SpatialTree::with_viewport();
        let mut index = HitIndex::new();
        let mut rounded = HitEntry::new(frag(1), square(20.0));
        rounded.radii = Corners::uniform(Vec2::splat(DevicePx(8.0)));
        index.update(rounded.frag, rounded);
        let corner = Point::new(DevicePx(0.5), DevicePx(0.5));
        let edge = Point::new(DevicePx(10.0), DevicePx(0.5));
        assert!(index.hit(corner, &clips, &spatial).is_empty());
        assert_eq!(index.hit(edge, &clips, &spatial).as_slice(), &[frag(1)]);

        index.update(frag(1), HitEntry::new(frag(1), square(20.0)));
        assert_eq!(
            index.hit(corner, &clips, &spatial).as_slice(),
            &[frag(1)],
            "the same corner of a square entry is on it"
        );
    }
}
