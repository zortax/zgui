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
use zgui_geom::Matrix4;

use crate::spatial::id::SpatialId;
use crate::spatial::space::SpatialTree;

/// The matrix every live coordinate system resolves to, in slot order.
///
/// Dense: a slot no node occupies holds the identity, so the array can be handed to a shader that
/// indexes it without bounds logic and without a hole reading as something visible. The occupancy
/// counter is kept beside each matrix so a name from a frame ago is *refused* rather than answered
/// out of a slot its box no longer owns.
#[derive(Clone, Debug, Default)]
pub struct Placements {
    /// One entry per slot, up to the highest live slot.
    slots: Vec<Slot>,
}

/// What one slot resolved to, and for whom.
#[derive(Clone, Copy, Debug)]
struct Slot {
    /// Which occupant of the slot this answer belongs to, or `None` where nothing is live.
    occupant: Option<Generation>,
    /// The matrix mapping that occupant's coordinates onto the device.
    matrix: Matrix4,
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            occupant: None,
            matrix: Matrix4::IDENTITY,
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
    pub const EMPTY: Self = Self { slots: Vec::new() };

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
    /// Every slot is rewritten because resolving is a walk to the root: one node moving changes the
    /// answer for every node below it, and finding out which those are costs more than answering
    /// them. The array is one matrix per *coordinate system*, which a document has few of — a
    /// thousand identical rows share one — so this is not the copy of every distinct matrix that
    /// interning made it.
    pub fn take(&mut self, tree: &SpatialTree) {
        self.slots.clear();
        self.slots
            .extend(tree.slots().map(|held| Self::resolved(tree, held)));
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
        let live = tree.slots().len();
        self.slots.resize(live, Slot::default());
        for (slot, held) in tree.slots().enumerate() {
            let fresh = Self::resolved(tree, held);
            let place = &mut self.slots[slot];
            if let Some(id) = held
                && fresh.occupant.is_some()
                && (place.occupant != fresh.occupant || place.matrix != fresh.matrix)
            {
                moved(id);
            }
            *place = fresh;
        }
    }

    /// What one slot resolves to, and for whom.
    fn resolved(tree: &SpatialTree, held: Option<SpatialId>) -> Slot {
        match held.and_then(|id| Some((id, tree.resolve(id)?))) {
            Some((id, matrix)) => Slot {
                occupant: Some(id.generation()),
                matrix,
            },
            None => Slot::default(),
        }
    }

    /// The matrix `id` names, or `None` when the slot has moved on to another occupant.
    pub fn get(&self, id: SpatialId) -> Option<&Matrix4> {
        let slot = self.slots.get(id.index() as usize)?;
        (slot.occupant == Some(id.generation())).then_some(&slot.matrix)
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
