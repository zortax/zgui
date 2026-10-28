//! The descent itself: one box's pieces, then everything below it.
//!
//! Read beside [`super`], which states what this walk is allowed to skip and what it is not, and
//! beside [`duty`](super::duty), which is how one descent is asked for one of its two duties.

use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Size};
use zgui_profile::{Counter, counter};
use zgui_scene::ClipId;

use crate::fragment::build::Tables;
use crate::fragment::diff::dirty::FrameDirty;
use crate::fragment::diff::rigid::duty::Duty;
use crate::fragment::hit::HitIndex;
use crate::fragment::{FragKey, FragmentFlags, clip};
use crate::tree::store::LayoutStore;

/// The state the offsetting walk carries.
///
/// Deliberately a different type from [`Pass`]: this walk holds no damage set and no restack flag,
/// because a subtree that only moved raises its damage once at the top and can produce no fragment
/// that has no place in painting order.
pub(super) struct Walk<'a, 'b, D: FrameDirty> {
    /// The boxes and their fragments.
    store: &'a mut LayoutStore,
    /// The index kept in step.
    hit: &'a mut HitIndex,
    /// Where clips are interned.
    tables: &'a mut Tables<'b>,
    /// Where the accessibility marks are recorded.
    dirty: &'a mut D,
    /// The offset every piece takes.
    by: Size<DevicePx, Device>,
    /// How far the offsets above the subtree scrolled, which every box's composed shift takes.
    scrolled: Size<DevicePx, Device>,
    /// Device pixels per CSS pixel.
    scale: f32,
}

impl<'a, 'b, D: FrameDirty> Walk<'a, 'b, D> {
    /// The walk over one subtree, carried by `moved`.
    pub(super) fn over(
        store: &'a mut LayoutStore,
        hit: &'a mut HitIndex,
        tables: &'a mut Tables<'b>,
        dirty: &'a mut D,
        moved: super::Move,
        scale: f32,
    ) -> Self {
        Self {
            store,
            hit,
            tables,
            dirty,
            by: moved.size(),
            scrolled: Size::new(DevicePx(moved.scrolled.0), DevicePx(moved.scrolled.1)),
            scale,
        }
    }

    /// Offsets one box's pieces and everything below it.
    ///
    /// `clip` is the chain this box itself is drawn under, already moved. `T` is which of the
    /// walk's duties this descent carries out; a descent that does not move anything also does not
    /// count the boxes it reached, so a subtree offset in four descents reports the same number of
    /// visited boxes as one offset in a single descent.
    pub(super) fn subtree<T: Duty>(
        &mut self,
        key: BoxKey,
        clip: ClipId,
        shift: Size<DevicePx, Device>,
    ) {
        self.subtree_from::<T>(key, clip, shift, true);
    }

    /// The descent, told whether it is still above the first element of the moved subtree.
    ///
    /// The accessibility tree measures every node from the nearest ancestor that carries a
    /// transform, and a subtree carried as one piece keeps every such distance inside it. What
    /// moved, as that tree states it, is the topmost element on each path down from the moved
    /// box — the frontier — so that is what the walk reports, and nothing below it.
    fn subtree_from<T: Duty>(
        &mut self,
        key: BoxKey,
        clip: ClipId,
        shift: Size<DevicePx, Device>,
        frontier: bool,
    ) {
        if T::MOVES {
            counter::bump(Counter::NodesVisited);
            counter::bump(Counter::FragmentBoxesVisited);
            counter::bump(Counter::BoxesShifted);
            // The shift is what the scrolls above the box add up to. A layout move that came
            // with a scroll carries the scroll as well, so the shift takes that part of it.
            if self.scrolled != Size::new(DevicePx(0.0), DevicePx(0.0)) {
                let state = self.store.state_mut(key);
                state.composed_shift.0 += self.scrolled.width.0;
                state.composed_shift.1 += self.scrolled.height.0;
            }
            // What this walk is entitled to claim: every box it carries was composed from the
            // result the engine holds for it, so carrying it is the same as composing it again.
            debug_assert!(
                self.store
                    .state(key)
                    .is_none_or(|state| state.unrounded == state.composed),
                "a subtree was carried whose engine result its fragments were not composed from"
            );
        }

        let inner = self.offset_own::<T>(key, clip, shift);
        self.offset_lines::<T>(key, inner);
        let mut frontier = frontier;
        if T::INDEXES && frontier {
            frontier = !self.note_move(key);
        }

        // What this box's own children are carried by, which is what it is carried by less
        // whatever it is itself scrolled to. The box already holds that number — it is what its
        // contents were last composed against — and it has just been moved along with everything
        // else, so it is read back rather than resolved again.
        let inner_shift = self.store.state(key).map_or(shift, |state| {
            Size::new(
                DevicePx(state.composed_shift.0),
                DevicePx(state.composed_shift.1),
            )
        });

        // Read one child at a time rather than cloning the list. The descent needs `&mut self`, so
        // the list cannot be borrowed across it — but the borrow ends at the end of each `let`, and
        // an arena lookup per child is cheaper than an allocation and a copy per *box*, which is
        // what a glide of a thousand frames was paying for every box it moved.
        //
        // Sound because nothing this walk does reshapes the box tree: it writes fragments, box
        // state, clip chains, the hit index and the accessibility marks, and none of those is a
        // child list. A descent that could add or remove a box would have to take a copy again.
        let mut position = 0;
        while let Some(&child) = self.store.node(key).children.get(position) {
            self.subtree_from::<T>(child, inner, inner_shift, frontier);
            position += 1;
        }
    }

    /// Offsets the box's own piece and returns the chain its contents are drawn under.
    ///
    /// The chain is derived from the moved padding box by the same call the composing walk makes,
    /// so a box that clips issues the identifier that walk would have issued and a box that does
    /// not passes its own chain straight down.
    ///
    /// A descent that does not move anything has no chain to derive and none to hand down: the
    /// piece already holds the one it is drawn under, so an indexing descent reads it off the piece
    /// rather than deriving it a second time.
    fn offset_own<T: Duty>(
        &mut self,
        key: BoxKey,
        clip: ClipId,
        shift: Size<DevicePx, Device>,
    ) -> ClipId {
        let Some(&frag) = self.store.fragments_of_box(key).first() else {
            return clip;
        };
        if !T::MOVES {
            if T::INDEXES
                && let Some(held) = self.store.fragment(frag).map(|piece| piece.clip)
            {
                self.reindex(frag, held);
            }
            return clip;
        }
        let by = self.by;
        let Some(fragment) = self.store.fragment_mut(frag) else {
            return clip;
        };
        fragment.border_box = fragment.border_box.translate(by);
        fragment.ink = fragment.ink.translate(by);
        fragment.local_ink = fragment.local_ink.translate(by);
        fragment.subtree_ink = fragment.subtree_ink.translate(by);
        fragment.clip = clip;
        let clips = fragment.flags.contains(FragmentFlags::CLIPS_CHILDREN);
        let padding_box = fragment.padding_box();
        let border = fragment.border;
        // The space this box's fragments draw under, which is the space its padding box —
        // and therefore the clip it imposes below — is measured in.
        let space = fragment
            .transform
            .unwrap_or(zgui_scene::SpatialId::VIEWPORT);
        if T::INDEXES {
            self.reindex(frag, clip);
        }
        if !clips {
            return clip;
        }
        let scale = self.scale;
        let style = &self.store.node(key).style;
        clip::chain_for_children(
            self.tables.clips,
            clip,
            style,
            self.store.custom_properties(key, style),
            padding_box,
            border,
            scale,
            shift,
            space,
            zgui_scene::PropertyOwner::of(key),
        )
    }

    /// Offsets the lines of an inline formatting context, which are drawn under the box's own
    /// clip rather than under the one it was given.
    fn offset_lines<T: Duty>(&mut self, key: BoxKey, inner: ClipId) {
        // Indexed rather than collected, for the same reason the child descent is: the loop body
        // needs `&mut self`, and one arena lookup per line beats an allocation per box. The count
        // is read once because this walk adds and removes no fragments — it only moves them.
        let count = self.store.fragments_of_box(key).len();
        let by = self.by;
        for position in 1..count {
            let Some(&frag) = self.store.fragments_of_box(key).get(position) else {
                break;
            };
            if !T::MOVES {
                if T::INDEXES
                    && let Some(held) = self.store.fragment(frag).map(|piece| piece.clip)
                {
                    self.reindex(frag, held);
                }
                continue;
            }
            let Some(fragment) = self.store.fragment_mut(frag) else {
                continue;
            };
            fragment.border_box = fragment.border_box.translate(by);
            fragment.ink = fragment.ink.translate(by);
            fragment.local_ink = fragment.local_ink.translate(by);
            fragment.subtree_ink = fragment.subtree_ink.translate(by);
            fragment.clip = inner;
            if T::INDEXES {
                self.reindex(frag, inner);
            }
        }
    }

    /// Moves one fragment's hit entry to where the fragment now is.
    ///
    /// Built from the entry already held rather than from the fragment and its style, because the
    /// only two things that differ are the two rectangles and the chain: a piece that moved has the
    /// same corner radii, the same painting order and the same answer about whether it takes
    /// pointer events at all.
    fn reindex(&mut self, frag: FragKey, clip: ClipId) {
        if self.hit.carry_by(frag, self.by, clip) {
            return;
        }
        // A piece with no entry has never been indexed, and the comparing walk would build one
        // for it here. Nothing about that is cheaper on this path, and skipping it would leave
        // a fragment that answers no hit at all.
        if let Some(built) = crate::fragment::hit::entry_for(self.store, frag, 0, self.scale) {
            self.hit.carry(frag, built);
        }
    }

    /// Records that the element this box came from moved, answering whether there was one.
    ///
    /// An accessibility node's geometry is stated relative to the nearest ancestor that carries a
    /// transform, so the element at the top of a moved subtree is the one whose statement is now
    /// wrong, whatever it declares: a plain row that moved carries the transform its cells are
    /// measured from. A box that came from no element — an anonymous one — reports nothing, and
    /// answers `false` so the walk goes on looking for the elements below it.
    ///
    /// It is reported as a *move* and not as a general obligation, which is the whole of what this
    /// path is entitled to claim: the subtree owes no work, so its semantics, its name, its
    /// relations, its actions and its child list are the ones already published, and the only thing
    /// about it that is now different is where it is. See [`FrameDirty::moved`].
    ///
    /// Nothing else is recorded, and the omission is deliberate. The pass records what it has just
    /// finished doing to a fragment under the same phases that made it enter — and retires those
    /// phases itself, in the same call, before anything downstream reads a mark. Writing them here
    /// would be a walk to every ancestor of every moved box to raise obligations the next few lines
    /// clear again.
    fn note_move(&mut self, key: BoxKey) -> bool {
        let Some(record) = self.store.get(key) else {
            return false;
        };
        // A run of text comes from a text node, which is no accessibility node: what an
        // assistive technology holds for it is its element's, and that element is above.
        if record.kind == crate::node::kind::BoxKind::TextRun {
            return false;
        }
        let Some(node) = record.source else {
            return false;
        };
        self.dirty.moved(Some(node));
        true
    }
}
