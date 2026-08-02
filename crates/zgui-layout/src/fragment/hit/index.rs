//! The index that answers what is under a point.

use smallvec::SmallVec;
use zgui_arena::SlotVec;
use zgui_geom::{Device, DevicePx, Point};
use zgui_profile::{Counter, counter};
use zgui_scene::{ClipTable, SpatialTree};

use crate::fragment::FragKey;
use crate::fragment::hit::entry::HitEntry;
use crate::fragment::hit::rtree::{Carried, Forest, Placed};
use crate::fragment::hit::transform;
use crate::tree::store::LayoutStore;

/// How much of the index may be touched one entry at a time before rebuilding it is cheaper.
///
/// A quarter is the point where the hierarchy has been reshaped enough that a fresh build produces
/// a better one than the incremental inserts did. Below it, rebuilding costs the whole document to
/// service work proportional to what moved.
pub const CHURN_FRACTION: usize = 4;

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
    /// One entry per fragment.
    entries: SlotVec<FragKey, HitEntry>,
    /// One spatial hierarchy per coordinate system, over the envelopes of the entries in it.
    forest: Forest,
    /// Entries touched since the last bulk build.
    churn: u32,
    /// The entries written by [`HitIndex::carry`] that the hierarchy has not been told about yet.
    ///
    /// Held on the index rather than raised at each call site so that the buffer outlives the frame
    /// and a scroll allocates nothing for it.
    carried: Vec<Carried>,
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
    pub fn entry(&self, frag: FragKey) -> Option<&HitEntry> {
        self.entries.get(frag)
    }

    /// Every fragment the index holds an entry for, in no particular order.
    pub fn indexed(&self) -> impl Iterator<Item = FragKey> + '_ {
        self.entries.iter().map(|(_, entry)| entry.frag)
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
        self.carried.push(Carried {
            frag,
            space: entry.space,
            bounds: entry.envelope,
        });
        self.entries.insert(frag, entry);
        counter::bump(Counter::HitEntriesUpdated);
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
    fn write(&mut self, frag: FragKey, entry: HitEntry) -> Placed {
        let placed = self.forest.place(frag, entry.space, entry.envelope);
        self.entries.insert(frag, entry);
        counter::bump(Counter::HitEntriesUpdated);
        placed
    }

    /// Takes one fragment out.
    pub fn remove(&mut self, frag: FragKey) {
        if self.entries.remove(frag).is_some() {
            self.forest.remove(frag);
            self.churn = self.churn.saturating_add(1);
        }
    }

    /// Whether the index has been touched enough that rebuilding it is the cheaper option.
    pub fn should_rebuild(&self) -> bool {
        !self.entries.is_empty() && self.churn as usize > self.entries.len() / CHURN_FRACTION
    }

    /// Builds the whole index again from the fragment tree.
    ///
    /// For when painting order itself moved — a `z-index` change restacks the document and every
    /// entry's order may be different — or when so much has been updated one entry at a time that
    /// the hierarchy is no longer a good one.
    pub fn rebuild(&mut self, store: &LayoutStore, scale: f32) {
        self.entries.clear();
        self.forest.clear();
        self.churn = 0;
        // A run that had not been settled named entries this build is about to replace, and
        // settling it afterwards would file rectangles for fragments the hierarchy has just been
        // told about properly.
        self.carried.clear();
        counter::bump(Counter::HitIndexRebuilds);
        let Some(root) = store.root() else {
            return;
        };
        let mut order = 0;
        for box_ in crate::fragment::stacking::paint_order(store, root) {
            for &frag in store.fragments_of_box(box_) {
                let Some(entry) = crate::fragment::hit::entry_for(store, frag, order, scale) else {
                    continue;
                };
                self.forest.insert(frag, entry.space, entry.envelope);
                self.entries.insert(frag, entry);
                order += 1;
            }
        }
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
        let mut hits: SmallVec<[(zgui_scene::DrawOrder, FragKey); 8]> = SmallVec::new();
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
                if !entry.covers(local) {
                    continue;
                }
                // The clip chain is tested in the space it was measured in, which is this
                // fragment's own unless the fragment carries a transform: the clip belongs to
                // whichever ancestor imposed it, and a clip tested in the wrong space is a clip
                // applied in the wrong place on the screen.
                let clipped = if entry.clip_space == space {
                    local
                } else {
                    let Some(mapped) = transform::into_local(point, entry.clip_space, spatial)
                    else {
                        continue;
                    };
                    mapped
                };
                if !clipped_in(clips, entry.clip, clipped) {
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

/// Whether a point survives every link of a clip chain.
fn clipped_in(clips: &ClipTable, clip: zgui_scene::ClipId, point: Point<DevicePx, Device>) -> bool {
    if clip.is_root() {
        return true;
    }
    for link in clips.links(clip) {
        match link {
            zgui_scene::ClipLink::RoundedRect { rect, radii, .. } => {
                if !crate::fragment::hit::entry::covers_rounded(rect, radii, point) {
                    return false;
                }
            }
            // A sampled mask decides coverage per pixel, which is a question for whatever holds the
            // coverage tile. Its rectangle is the most this index can test.
            zgui_scene::ClipLink::Mask { tile, .. } => {
                let bounds = tile.bounds;
                if point.x.0 < bounds.origin.x as f32
                    || point.y.0 < bounds.origin.y as f32
                    || point.x.0 > (bounds.origin.x + bounds.size.width) as f32
                    || point.y.0 > (bounds.origin.y + bounds.size.height) as f32
                {
                    return false;
                }
            }
        }
    }
    true
}
