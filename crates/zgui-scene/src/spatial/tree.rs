//! Property nodes: storage whose names outlive the values held under them.

use rustc_hash::FxHashMap;
use zgui_arena::{ChunkArena, DomainId};

use crate::spatial::id::{PropertyId, PropertyOwner};

/// What a property tree needs of the values it stores.
///
/// A tree is a tree because its values point at each other, and the tree itself has to be able to
/// follow that link — to resolve a node against the ones above it — without knowing what else the
/// value holds.
pub trait PropertyNode<I> {
    /// The node this one is expressed in terms of, or `None` for a root.
    fn parent(&self) -> Option<I>;
}

/// Nodes named by the boxes that establish them, kept across frames.
///
/// Two things follow from naming a node after a box rather than after its value, and both of them
/// are the point. A box that is relaid out, restyled or has its fragments rebuilt is given back the
/// name it had, so an animation writes a value sixty times a second under one unchanging name. And
/// a box that establishes nothing of its own is simply given the name of the node above it, so a
/// thousand identical rows name one node between them — the same deduplication interning a value
/// provides, arrived at from structure instead.
///
/// Names carry an occupancy counter, which is load-bearing rather than tidy. A slot that comes back
/// and is handed to an unrelated box must not compare equal to the name something recorded before,
/// because that comparison is what decides whether output cached under the old name may be reused,
/// and the two names would otherwise be one word apiece and identical.
///
/// A removed node stays readable until [`PropertyTree::recycle`] ends the frame, so passes that
/// hold names across one another need no coordination beyond running inside the same frame.
#[derive(Debug)]
pub struct PropertyTree<I: PropertyId<Node = N>, N: PropertyNode<I>> {
    /// Every node, in storage whose addresses hold still and whose slots carry a counter.
    nodes: ChunkArena<N>,
    /// The node each box established, so that a box seen again is given back the name it had.
    named: FxHashMap<PropertyOwner, I>,
    /// The name occupying each slot, so a slot number can be turned back into a name.
    ///
    /// A dense buffer is addressed by slot and an instance in it carries nothing but the slot, so
    /// something reading that buffer back — a renderer filling it, a transcript printing what a
    /// primitive was drawn through — has only the slot to go on. Kept here rather than searched
    /// for, because searching is a walk of every name per lookup.
    by_slot: Vec<Option<I>>,
    /// The slots given back this frame, which stop resolving when it ends.
    released: Vec<u32>,
    /// Whether any node's value or occupancy changed since the last [`PropertyTree::recycle`].
    ///
    /// A reader that filed something under a name has to find out when what that name resolves to
    /// stops being what it filed it under, and finding out costs a comparison per node. This is the
    /// one-word answer to "is there anything to compare" — false on every frame that established
    /// nothing new and rewrote nothing, which is every frame of a document that is not animating,
    /// scrolling stickily or being restructured.
    written: bool,
}

impl<I: PropertyId<Node = N>, N: PropertyNode<I>> PropertyTree<I, N> {
    /// An empty tree that mints names in `domain`.
    ///
    /// The domain says which document owns the tree and which of that document's arenas it is, so
    /// a name minted here can never resolve inside another arena or another document.
    pub fn new(domain: DomainId) -> Self {
        Self {
            nodes: ChunkArena::new(domain),
            named: FxHashMap::default(),
            by_slot: Vec::new(),
            released: Vec::new(),
            written: false,
        }
    }

    /// The arena this tree is.
    pub const fn domain(&self) -> DomainId {
        self.nodes.domain()
    }

    /// How many nodes are live.
    pub const fn len(&self) -> u32 {
        self.nodes.len()
    }

    /// Whether no node is live.
    pub const fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The name `owner` established, if it still holds one.
    pub fn of(&self, owner: PropertyOwner) -> Option<I> {
        self.named.get(&owner).copied()
    }

    /// Every live name, in no particular order.
    ///
    /// The names rather than the nodes, because a node is only meaningful with the name that
    /// resolves it: what a caller does with one of these is fold up from it.
    pub fn ids(&self) -> impl Iterator<Item = I> + '_ {
        self.named.values().copied()
    }

    /// Names a node after the box that establishes it, giving back the name that box already had.
    ///
    /// The value is overwritten and the name is not, which is what an animating transform needs:
    /// what changed is where the coordinate system is, not which coordinate system it is.
    ///
    /// A node is established after the one it names as its parent, so the tree cannot come to hold
    /// a cycle by being built. Re-establishing a node under a parent that is below it would make
    /// one, and the walk up from either would not terminate; a walk that descends before it
    /// establishes never asks for that.
    ///
    /// Overwriting with the value already held is recorded as no write at all, which is what makes
    /// [`PropertyTree::written_since_recycle`] worth asking: the overwhelming majority of frames
    /// re-establish the same coordinate systems with the same matrices, and a reader that has to
    /// notice movement can then skip looking for it entirely.
    ///
    /// # Panics
    ///
    /// In debug builds, panics if the node names a parent that is not live. Panics in any build if
    /// a name this tree still hands out has stopped resolving, which is this type's own invariant
    /// rather than anything a caller can arrange: a name and the node under it are given up in one
    /// call and come back in one call.
    pub fn establish(&mut self, owner: PropertyOwner, node: N) -> I
    where
        N: PartialEq,
    {
        debug_assert!(
            node.parent().is_none_or(|parent| self.contains(parent)),
            "a node is established under a node that is already there",
        );
        match self.named.get(&owner).copied() {
            Some(id) => {
                // A name and the node under it are given up together, so an indexed name resolves.
                let held = self
                    .nodes
                    .get_mut(id.key())
                    .expect("a name this tree still hands out resolves to a node");
                if *held != node {
                    *held = node;
                    self.written = true;
                }
                id
            }
            None => {
                let fresh = I::from_key(self.nodes.insert(node));
                self.named.insert(owner, fresh);
                let slot = fresh.key().index() as usize;
                if slot >= self.by_slot.len() {
                    self.by_slot.resize(slot + 1, None);
                }
                self.by_slot[slot] = Some(fresh);
                self.written = true;
                fresh
            }
        }
    }

    /// Whether anything has been established, rewritten or given back since the frame began.
    ///
    /// False is the useful answer and the common one: a document that is not animating, not
    /// scrolling something sticky and not being restructured re-establishes every coordinate system
    /// it had with the value it had, and nothing that resolves through this tree can have moved. A
    /// reader that keeps its own record of where things were may then skip comparing entirely,
    /// which is a comparison per coordinate system per frame it does not pay.
    pub const fn written_since_recycle(&self) -> bool {
        self.written
    }

    /// The node a name refers to, or `None` if that name is no longer live.
    pub fn get(&self, id: I) -> Option<&N> {
        self.nodes.get(id.key())
    }

    /// The node a name refers to, for writing.
    pub fn get_mut(&mut self, id: I) -> Option<&mut N> {
        self.nodes.get_mut(id.key())
    }

    /// Records that a node's value was written through [`PropertyTree::get_mut`].
    ///
    /// The one thing a borrow of the value cannot do for itself. A writer that takes the value out
    /// and changes it has to say so, or every reader that skips comparing on
    /// [`PropertyTree::written_since_recycle`] skips the frame the write happened on.
    pub fn note_written(&mut self) {
        self.written = true;
    }

    /// Whether a name still refers to the node it was handed out for.
    pub fn contains(&self, id: I) -> bool {
        self.nodes.contains_key(id.key())
    }

    /// Gives back the node a box established, and the name with it.
    ///
    /// The value stays readable until the frame ends, and the slot is withheld until then, so
    /// nothing that already holds the name is handed a different node part-way through a frame.
    /// Nodes below the released one keep naming it and stop resolving once the frame ends, which
    /// is why a subtree is released from the bottom.
    pub fn release(&mut self, owner: PropertyOwner) -> Option<I> {
        let id = self.named.remove(&owner)?;
        self.nodes.remove(id.key());
        self.released.push(id.key().index());
        self.written = true;
        Some(id)
    }

    /// The name occupying `slot`, if one still does.
    ///
    /// What turns a primitive's slot number back into the name it was pushed with, which is the
    /// only direction a dense buffer leaves open.
    pub fn at(&self, slot: u32) -> Option<I> {
        *self.by_slot.get(slot as usize)?
    }

    /// Every slot in order, with the name occupying it.
    ///
    /// The order a dense buffer is written in, so a caller filling one walks this and writes as it
    /// goes rather than sorting afterwards.
    pub fn slots(&self) -> impl ExactSizeIterator<Item = Option<I>> + '_ {
        self.by_slot.iter().copied()
    }

    /// Ends the frame: drops what was released during it and offers the slots back.
    ///
    /// Each slot moves on to its next occupancy counter as its value is dropped, so every name into
    /// this frame's releases stops resolving here, all at once.
    pub fn recycle(&mut self) {
        for slot in self.released.drain(..) {
            self.by_slot[slot as usize] = None;
        }
        self.nodes.recycle();
        self.written = false;
    }

    /// Folds over `id` and every node above it, from `id` upwards.
    ///
    /// `None` if the chain reaches a name that is no longer live, which is the difference between
    /// a node that resolves to nothing and one that resolves to the identity: a released ancestor
    /// leaves no answer at all rather than a plausible one.
    pub fn fold_up<T>(&self, id: I, init: T, mut step: impl FnMut(T, &N) -> T) -> Option<T> {
        let mut folded = init;
        let mut next = Some(id);
        while let Some(current) = next {
            let node = self.get(current)?;
            folded = step(folded, node);
            next = node.parent();
        }
        Some(folded)
    }
}
