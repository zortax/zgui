//! Every coordinate system resolved to a matrix, addressed by slot.
//!
//! A node is named by structure and resolved by walking up to the root, which is the right shape
//! for a name that has to survive a frame and the wrong shape for two readers that want the answer
//! for every node at once. A renderer wants exactly that — one matrix per slot, in a buffer the
//! shader indexes — and so does anything answering questions about the frame that was drawn. This
//! is that array, computed once from the tree.
//!
//! # Why this is not a copy of a table
//!
//! The thing it replaces held a *copy of the tree's values* so that an identifier from the drawn
//! frame would not be resolved against the values of the frame being built. A content-interned
//! identifier needed that, because the same slot in two frames could hold two unrelated matrices.
//! A structural name does not: slot five is the same box's coordinate system in every frame it is
//! live in, so what a reader between frames needs is not a copy of the tree but the tree's answers,
//! which is what a renderer computes anyway.
//!
//! ```
//! use zgui_geom::Matrix4;
//! use zgui_scene::{OwnSpace, Placements, PropertyOwner, SpatialTree};
//!
//! let mut tree = SpatialTree::with_viewport();
//! let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
//! let moved = Matrix4::translation(8.0, 0.0, 0.0);
//! let card = tree.space_of(tree.viewport(), owner, OwnSpace::of(Some(moved), None, false));
//!
//! let placements = Placements::of(&tree);
//! assert_eq!(placements.get(card), Some(&moved));
//! assert_eq!(placements.get(tree.viewport()), Some(&Matrix4::IDENTITY));
//! ```

use zgui_arena::Generation;
use zgui_geom::{Device, DevicePx, Matrix4, Rect};

use crate::spatial::id::SpatialId;
use crate::spatial::space::SpatialTree;

/// The matrix every live coordinate system resolves to, in slot order.
///
/// Dense: a slot no node occupies holds the identity, so the array can be handed to a shader that
/// indexes it without bounds logic and without a hole reading as something visible. The occupancy
/// counter is kept beside each matrix so a name from a frame ago is *refused* rather than answered
/// out of a slot its box no longer owns.
#[derive(Debug, Default)]
pub struct Placements {
    /// One entry per slot, up to the highest live slot.
    slots: Vec<Slot>,
    /// The tree's write stamp when these answers were taken, or zero before the first take.
    ///
    /// No tree holds the stamp zero. See [`SpatialTree::writes`](crate::PropertyTree::writes).
    stamp: u64,
    /// Scratch for one take: per slot, the answer for its occupant once it is known.
    known: Vec<Option<Option<Matrix4>>>,
    /// Scratch for one take: the names from one node up to the first answer known.
    chain: Vec<(SpatialId, Matrix4)>,
}

/// A copy of the answers and their stamp, with empty scratch.
impl Clone for Placements {
    fn clone(&self) -> Self {
        Self {
            slots: self.slots.clone(),
            stamp: self.stamp,
            known: Vec::new(),
            chain: Vec::new(),
        }
    }
}

/// What one slot resolved to, and for whom.
#[derive(Clone, Copy, Debug)]
struct Slot {
    /// Which occupant of the slot this answer belongs to, or `None` where nothing is live.
    occupant: Option<Generation>,
    /// The matrix mapping that occupant's coordinates onto the device.
    matrix: Matrix4,
    /// Whether that matrix moves nothing, decided once when it is resolved.
    ///
    /// Nearly every coordinate system in a document resolves to the identity — a space is
    /// established per scroll container and per transformable box, and most never move — and a
    /// consumer placing a rectangle per fragment per frame wants to skip the arithmetic rather
    /// than perform a transform that lands where it started.
    identity: bool,
    /// The translation in the plane that matrix is, when it is one, decided at the same time.
    ///
    /// Nearly every coordinate system that does move is a scroll container's content, which only
    /// translates. A consumer placing a rectangle per fragment then adds four numbers and does not
    /// test the matrix again.
    plane: Option<(f32, f32)>,
}

impl Slot {
    /// What a slot holding `held` resolves to, given that occupant's answer.
    fn of(held: Option<SpatialId>, answer: Option<Matrix4>) -> Self {
        match held.zip(answer) {
            Some((id, matrix)) => Self {
                occupant: Some(id.generation()),
                matrix,
                identity: matrix == Matrix4::IDENTITY,
                plane: matrix.plane_translation(),
            },
            None => Self::default(),
        }
    }
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            occupant: None,
            matrix: Matrix4::IDENTITY,
            identity: true,
            plane: Some((0.0, 0.0)),
        }
    }
}

impl Placements {
    /// The answers for a frame that has not been composed, which resolve nothing.
    ///
    /// Every name is refused, so a caller reading geometry through this is left with the rectangle
    /// in the space it was measured in. That is the honest answer before anything has been drawn,
    /// and it is a constant so that a caller which has no frame to hand — a projection under test,
    /// a store being read outside a window — can borrow one without owning it.
    pub const EMPTY: Self = Self {
        slots: Vec::new(),
        stamp: 0,
        known: Vec::new(),
        chain: Vec::new(),
    };

    /// An empty set of answers, which resolves nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolves every live node of `tree`.
    pub fn of(tree: &SpatialTree) -> Self {
        let mut placements = Self::new();
        placements.take(tree);
        placements
    }

    /// Replaces these answers with `tree`'s, reusing the storage.
    ///
    /// Every slot is rewritten because one node moving changes the answer for every node below it,
    /// and finding out which those are costs more than answering them. Each slot is composed once,
    /// from its parent's answer, so a take costs one product per coordinate system. The array is
    /// one matrix per *coordinate system*, which a document has few of — a thousand identical rows
    /// share one — so this is not the copy of every distinct matrix that interning made it.
    ///
    /// While the tree's [write stamp](crate::PropertyTree::writes) is the one these answers were
    /// taken at, no answer can have changed, and this does nothing.
    pub fn take(&mut self, tree: &SpatialTree) {
        if self.stamp == tree.writes() {
            return;
        }
        self.resolve_all(tree);
        self.slots.clear();
        self.slots.extend(
            tree.slots()
                .zip(&self.known)
                .map(|(held, known)| Slot::of(held, known.flatten())),
        );
        self.stamp = tree.writes();
    }

    /// The same, reporting every coordinate system whose answer is not the one it held.
    ///
    /// A structural name does not move when the matrix under it does, so a reader that filed
    /// something under a name — where a control is, in the numbers a consumer outside this process
    /// was given — cannot tell from the name alone that what it filed has gone stale. This is the
    /// one place both answers exist at once, and it is therefore the only place the difference can
    /// be observed without walking anything.
    ///
    /// A node whose own transform did not change is still reported when a node above it moved,
    /// because what is compared is what each name *resolves* to.
    ///
    /// ```
    /// use zgui_geom::Matrix4;
    /// use zgui_scene::{OwnSpace, Placements, PropertyOwner, SpatialTree};
    ///
    /// let mut tree = SpatialTree::with_viewport();
    /// let owner = |raw| PropertyOwner::new(raw).expect("a handle is never the empty word");
    /// let slid = |x| OwnSpace::of(Some(Matrix4::translation(x, 0.0, 0.0)), None, false);
    /// let card = tree.space_of(tree.viewport(), owner(2), slid(4.0));
    /// let label = tree.space_of(card, owner(3), slid(1.0));
    ///
    /// let mut placements = Placements::of(&tree);
    /// assert_eq!(tree.space_of(tree.viewport(), owner(2), slid(9.0)), card, "the same name");
    ///
    /// let mut moved = Vec::new();
    /// placements.take_noting_moves(&tree, &mut |id| moved.push(id));
    /// moved.sort_unstable();
    /// assert_eq!(moved, vec![card, label], "a label under a card that moved is somewhere else");
    /// ```
    pub fn take_noting_moves(&mut self, tree: &SpatialTree, moved: &mut dyn FnMut(SpatialId)) {
        self.take_noting_slots(tree, &mut |_, id| {
            if let Some(id) = id {
                moved(id);
            }
        });
    }

    /// The same, reporting every slot whose occupant or resolved matrix changed.
    ///
    /// Unlike [`Placements::take_noting_moves`], this also reports a slot that became vacant. That
    /// distinction matters to a dense GPU table: the stale matrix has to be replaced by the
    /// identity even though there is no longer an id to report.
    pub fn take_noting_slots(
        &mut self,
        tree: &SpatialTree,
        changed: &mut dyn FnMut(u32, Option<SpatialId>),
    ) {
        if self.stamp == tree.writes() {
            return;
        }
        self.resolve_all(tree);
        let live = tree.slots().len();
        self.slots.resize(live, Slot::default());
        for ((slot, held), known) in tree.slots().enumerate().zip(&self.known) {
            let fresh = Slot::of(held, known.flatten());
            let place = &mut self.slots[slot];
            if place.occupant != fresh.occupant || place.matrix != fresh.matrix {
                changed(slot as u32, held);
            }
            *place = fresh;
        }
        self.stamp = tree.writes();
    }

    /// Fills `known` with the answer for every slot's occupant, one product per slot.
    ///
    /// Each answer is the node's own transform followed by its parent's answer, which is the order
    /// [`SpatialTree::resolve`] composes in. Every answer is therefore equal in bits to that one.
    fn resolve_all(&mut self, tree: &SpatialTree) {
        self.known.clear();
        self.known.resize(tree.slots().len(), None);
        for (slot, held) in tree.slots().enumerate() {
            if self.known[slot].is_some() {
                continue;
            }
            let answer = held.and_then(|id| self.resolve_one(tree, id));
            self.known[slot] = Some(answer);
        }
    }

    /// What `id` resolves to, from the nearest answer above it already known.
    ///
    /// Records the answer for every node on the way that occupies its slot.
    fn resolve_one(&mut self, tree: &SpatialTree, id: SpatialId) -> Option<Matrix4> {
        self.chain.clear();
        // `None` while the walk has found nothing above the chain: the chain then ends at a root.
        let mut above: Option<Option<Matrix4>> = None;
        let mut next = Some(id);
        while let Some(current) = next {
            if tree.at(current.index()) == Some(current)
                && let Some(&Some(answer)) = self.known.get(current.index() as usize)
            {
                above = Some(answer);
                break;
            }
            let Some(node) = tree.get(current) else {
                above = Some(None);
                break;
            };
            self.chain.push((current, node.local));
            next = node.parent;
        }
        while let Some((current, local)) = self.chain.pop() {
            let answer = match above {
                None => Some(local),
                Some(matrix) => matrix.map(|matrix| local.then(&matrix)),
            };
            if tree.at(current.index()) == Some(current) {
                self.known[current.index() as usize] = Some(answer);
            }
            above = Some(answer);
        }
        above.flatten()
    }

    /// Whether `other` holds the same occupant and the same matrix bits in every slot.
    pub(crate) fn same_as(&self, other: &Self) -> bool {
        let bits = |matrix: &Matrix4| matrix.columns.map(|column| column.map(f32::to_bits));
        self.slots.len() == other.slots.len()
            && self.slots.iter().zip(&other.slots).all(|(left, right)| {
                left.occupant == right.occupant
                    && left.identity == right.identity
                    && bits(&left.matrix) == bits(&right.matrix)
            })
    }

    /// The matrix `id` names, or `None` when the slot has moved on to another occupant.
    pub fn get(&self, id: SpatialId) -> Option<&Matrix4> {
        let slot = self.slots.get(id.index() as usize)?;
        (slot.occupant == Some(id.generation())).then_some(&slot.matrix)
    }

    /// The matrix `id` names, only where it moves anything.
    ///
    /// `None` both where the slot has moved on and where the answer is the identity: a caller
    /// transforming geometry treats the two the same, and the identity is the common case a
    /// per-fragment cull must not pay matrix arithmetic for.
    pub fn moves(&self, id: SpatialId) -> Option<&Matrix4> {
        let slot = self.slots.get(id.index() as usize)?;
        (slot.occupant == Some(id.generation()) && !slot.identity).then_some(&slot.matrix)
    }

    /// `rect`, measured in the coordinate system `id` names, where the device shows it.
    ///
    /// The rectangle as it is where the slot has moved on or the answer is the identity. Equal in
    /// bits to [`transformed_bounds`](zgui_geom::transformed_bounds) through
    /// [`Placements::moves`], and a translation in the plane costs no test of the matrix.
    ///
    /// ```
    /// use zgui_geom::{Device, DevicePx, Matrix4, Point, Rect, Size};
    /// use zgui_scene::{OwnSpace, Placements, PropertyOwner, SpatialTree};
    ///
    /// let mut tree = SpatialTree::with_viewport();
    /// let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
    /// let moved = Matrix4::translation(8.0, -3.0, 0.0);
    /// let card = tree.space_of(tree.viewport(), owner, OwnSpace::of(Some(moved), None, false));
    /// let rect: Rect<DevicePx, Device> =
    ///     Rect::new(Point::new(DevicePx(1.0), DevicePx(2.0)), Size::new(DevicePx(4.0), DevicePx(5.0)));
    ///
    /// let placements = Placements::of(&tree);
    /// assert_eq!(placements.place(card, rect), zgui_geom::transformed_bounds(&moved, rect));
    /// assert_eq!(placements.place(tree.viewport(), rect), rect);
    /// ```
    #[inline]
    pub fn place(&self, id: SpatialId, rect: Rect<DevicePx, Device>) -> Rect<DevicePx, Device> {
        let Some(slot) = self.slots.get(id.index() as usize) else {
            return rect;
        };
        if slot.occupant != Some(id.generation()) || slot.identity {
            return rect;
        }
        match slot.plane {
            Some((x, y)) => zgui_geom::translated_bounds(x, y, rect),
            None => zgui_geom::transformed_bounds(&slot.matrix, rect),
        }
    }

    /// Every slot's matrix, in slot order, with the identity where nothing is live.
    ///
    /// What a shader's storage buffer is filled from: a primitive names its coordinate system by
    /// slot, so the array has to be addressed the same way.
    pub fn matrices(&self) -> impl ExactSizeIterator<Item = Matrix4> + '_ {
        self.slots.iter().map(|slot| slot.matrix)
    }

    /// How many slots are addressed.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether nothing is addressed.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use zgui_geom::{Device, DevicePx, Matrix4, Point, Rect, Size, transformed_bounds};

    use super::Placements;
    use crate::{Anchoring, OwnSpace, PropertyOwner, SpatialId, SpatialNode, SpatialTree};

    /// A matrix as its sixteen bit patterns, so `-0.0` and NaN compare as stored.
    fn bits(matrix: &Matrix4) -> [[u32; 4]; 4] {
        matrix.columns.map(|column| column.map(f32::to_bits))
    }

    /// A transform of kind `kind % 5` with parameters `a` and `b`.
    fn transform(kind: u8, a: f32, b: f32) -> Matrix4 {
        let (sin, cos) = a.sin_cos();
        let rotation = Matrix4::from_columns([
            [cos, sin, 0.0, 0.0],
            [-sin, cos, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]);
        match kind % 5 {
            0 => Matrix4::translation(a * 37.3, b * -91.7, 0.0),
            1 => Matrix4::scale(a, b, 1.0),
            2 => rotation,
            3 => rotation.then(&Matrix4::translation(b * 13.1, a * 7.9, 0.0)),
            _ => Matrix4::perspective(b * 400.0 + 50.0).then(&Matrix4::scale(a, a, 1.0)),
        }
    }

    /// Asserts that every slot `placements` holds is the answer `tree` resolves, in every bit.
    fn assert_matches(placements: &Placements, tree: &SpatialTree) {
        assert_eq!(placements.len(), tree.slots().len());
        for (slot, held) in tree.slots().enumerate() {
            let direct = held.and_then(|id| tree.resolve(id));
            let kept = held.and_then(|id| placements.get(id));
            assert_eq!(kept.map(bits), direct.as_ref().map(bits), "slot {slot}");
            if let (Some(id), Some(direct)) = (held, direct) {
                // The order both compose in: the node's own transform, then its parent's answer.
                let node = tree.get(id).expect("a resolved name is live");
                let composed = match node.parent {
                    None => Some(node.local),
                    Some(parent) => tree.resolve(parent).map(|above| node.local.then(&above)),
                };
                assert_eq!(
                    composed.as_ref().map(bits),
                    Some(bits(&direct)),
                    "slot {slot}"
                );
            }
        }
    }

    proptest! {
        /// Every slot resolved from its parent's kept answer is the answer
        /// [`SpatialTree::resolve`] gives, through establishment, rewrites, release and reuse.
        #[test]
        fn a_slot_resolved_from_its_parent_is_the_direct_answer_in_every_bit(
            operations in prop::collection::vec(
                (0u8..10, any::<u16>(), 0u8..5, -3.0f32..3.0, -3.0f32..3.0),
                1..120,
            )
        ) {
            let mut tree = SpatialTree::with_viewport();
            let mut noted = Placements::new();
            let mut owners: Vec<PropertyOwner> = Vec::new();
            let mut next_owner = 100u64;
            for (op, pick, kind, a, b) in operations {
                match op {
                    // Establish a new node under a live one.
                    0..=3 => {
                        let mut live: Vec<SpatialId> = tree.ids().collect();
                        live.sort_unstable();
                        let parent = live[pick as usize % live.len()];
                        let owner = PropertyOwner::new(next_owner).expect("not the empty word");
                        next_owner += 1;
                        tree.establish(
                            owner,
                            SpatialNode {
                                parent: Some(parent),
                                local: transform(kind, a, b),
                                anchoring: Anchoring::Scrolling,
                            },
                        );
                        owners.push(owner);
                    }
                    // Rewrite a live node's own transform.
                    4 | 5 => {
                        if let Some(&owner) = owners.get(pick as usize % owners.len().max(1))
                            && let Some(id) = tree.of(owner)
                        {
                            tree.place(id, transform(kind, a, b));
                        }
                    }
                    // Give a node back; what was below it stops resolving at the frame's end.
                    6 => {
                        if !owners.is_empty() {
                            let owner = owners.swap_remove(pick as usize % owners.len());
                            tree.release(owner);
                        }
                    }
                    // Establish a live owner again, under another parent outside its subtree.
                    7 | 8 => {
                        if let Some(&owner) = owners.get(pick as usize % owners.len().max(1))
                            && let Some(id) = tree.of(owner)
                        {
                            let below = |mut at: Option<SpatialId>| {
                                while let Some(current) = at {
                                    if current == id {
                                        return true;
                                    }
                                    at = tree.get(current).and_then(|node| node.parent);
                                }
                                false
                            };
                            let mut outside: Vec<SpatialId> =
                                tree.ids().filter(|&other| !below(Some(other))).collect();
                            outside.sort_unstable();
                            if !outside.is_empty() {
                                let parent = outside[(pick as usize / 7) % outside.len()];
                                tree.establish(
                                    owner,
                                    SpatialNode {
                                        parent: Some(parent),
                                        local: transform(kind, b, a),
                                        anchoring: Anchoring::Scrolling,
                                    },
                                );
                            }
                        }
                    }
                    _ => tree.recycle(),
                }
                noted.take_noting_slots(&tree, &mut |_, _| {});
                assert_matches(&noted, &tree);
                assert_matches(&Placements::of(&tree), &tree);
            }
        }
    }

    #[test]
    fn a_take_reports_only_the_slots_a_write_changed() {
        let mut tree = SpatialTree::with_viewport();
        let owner = |raw| PropertyOwner::new(raw).expect("a handle is never the empty word");
        let slid = |x| OwnSpace::of(Some(Matrix4::translation(x, 0.0, 0.0)), None, false);
        let card = tree.space_of(tree.viewport(), owner(2), slid(4.0));
        let label = tree.space_of(card, owner(3), slid(1.0));

        let mut placements = Placements::new();
        let mut changed = Vec::new();
        placements.take_noting_slots(&tree, &mut |slot, _| changed.push(slot));
        assert_eq!(changed.len(), 3, "the first take reports every slot");

        changed.clear();
        tree.space_of(tree.viewport(), owner(2), slid(4.0));
        placements.take_noting_slots(&tree, &mut |slot, _| changed.push(slot));
        assert!(changed.is_empty(), "the same matrix again is no write");

        assert!(tree.place(card, Matrix4::translation(6.0, 0.0, 0.0)));
        placements.take_noting_slots(&tree, &mut |slot, _| changed.push(slot));
        changed.sort_unstable();
        assert_eq!(changed, vec![card.index(), label.index()]);
        assert_eq!(
            placements.get(label),
            Some(&Matrix4::translation(7.0, 0.0, 0.0))
        );
    }

    #[test]
    fn a_placed_rectangle_is_the_one_the_matrix_gives() {
        let mut tree = SpatialTree::with_viewport();
        let owner = |raw| PropertyOwner::new(raw).expect("a handle is never the empty word");
        let space = |matrix| OwnSpace::of(Some(matrix), None, false);
        let still = tree.space_of(tree.viewport(), owner(2), space(Matrix4::IDENTITY));
        let slid = tree.space_of(
            tree.viewport(),
            owner(3),
            space(Matrix4::translation(0.1, -22_000.0, 0.0)),
        );
        let scaled = tree.space_of(slid, owner(4), space(Matrix4::scale(2.0, 0.5, 1.0)));
        let placements = Placements::of(&tree);
        let bits = |rect: Rect<DevicePx, Device>| {
            [
                rect.origin.x,
                rect.origin.y,
                rect.size.width,
                rect.size.height,
            ]
            .map(|v| v.0.to_bits())
        };
        for rect in [
            Rect::new(
                Point::new(DevicePx(0.3), DevicePx(22_040.0)),
                Size::new(DevicePx(1265.0), DevicePx(22.0)),
            ),
            Rect::new(
                Point::new(DevicePx(-0.0), DevicePx(5.0)),
                Size::new(DevicePx(0.0), DevicePx(-1.0)),
            ),
            Rect::new(
                Point::new(DevicePx(f32::NAN), DevicePx(1.0)),
                Size::new(DevicePx(2.0), DevicePx(3.0)),
            ),
        ] {
            for id in [tree.viewport(), still, slid, scaled] {
                let expected = match placements.moves(id) {
                    Some(matrix) => transformed_bounds(matrix, rect),
                    None => rect,
                };
                assert_eq!(
                    bits(placements.place(id, rect)),
                    bits(expected),
                    "{id:?} {rect:?}"
                );
            }
        }
        // A name its box gave back answers nothing, for `place` as for `moves`.
        tree.release(owner(4));
        tree.release(owner(3));
        tree.recycle();
        let placements = Placements::of(&tree);
        let rect = Rect::new(
            Point::new(DevicePx(1.0), DevicePx(2.0)),
            Size::new(DevicePx(3.0), DevicePx(4.0)),
        );
        assert!(placements.moves(slid).is_none());
        assert_eq!(placements.place(slid, rect), rect);
    }
}
