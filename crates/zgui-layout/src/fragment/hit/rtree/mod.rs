//! A bounding-volume hierarchy over fragment rectangles.
//!
//! A hit test asks "what is under this point", and answering it by scanning every fragment costs
//! the whole document per pointer move. This is the structure that makes it cost the depth of the
//! tree instead: interior nodes hold the bounding rectangle of everything below them, so a subtree
//! whose rectangle misses the point is dismissed without being descended.
//!
//! It is a dynamic tree rather than a bulk-built one, because entries move one at a time — a
//! transform transition moves one row per tick and nothing else — and rebuilding for each of those
//! is exactly the cost the incremental path exists to avoid.
//!
//! # Why an entry is found by name
//!
//! Every entry's leaf is recorded as the entry is written, and every node knows the node above it.
//! Without those two links the only way to reach an entry is to descend from the root looking for
//! a node whose envelope contains the rectangle it was inserted with — which walks siblings that
//! merely overlap, allocates at every level to escape the borrow, and is wrong outright if the
//! caller's idea of the old rectangle has drifted from the tree's. With them, taking an entry out
//! costs the depth of the tree and allocates nothing.
//!
//! # Why a move is usually not a move
//!
//! A leaf's envelope covers up to [`MAX_ENTRIES`] neighbouring rectangles. An entry that shifts and
//! still lies inside its own leaf's envelope can be rewritten where it lies: every ancestor already
//! contains that envelope, so every ancestor already contains the new rectangle, and no envelope
//! anywhere needs touching. An entry that has left that envelope but still meets it has stayed
//! among the same neighbours, so it keeps its leaf and the envelopes above it are recomputed. Only
//! an entry that has gone somewhere else entirely is searched for a new home. See
//! [`place::at`](place::at) for the whole of that reasoning; between them the first two answers are
//! what a scrolled document does to this structure between two frames.

mod envelope;
pub(crate) mod forest;
mod home;
mod insert;
mod node;
mod place;
mod query;
mod remove;
mod settle;

use zgui_geom::{Device, DevicePx, Point, Rect};
use zgui_profile::{Counter, counter};
use zgui_scene::SpatialId;

use crate::fragment::FragKey;
use crate::fragment::hit::rtree::home::{Home, Homes};
use crate::fragment::hit::rtree::node::Node;

pub(crate) use crate::fragment::hit::rtree::forest::{Carried, Forest};
pub(crate) use crate::fragment::hit::rtree::node::{area, envelope_of};
pub(crate) use crate::fragment::hit::rtree::place::Placed;

/// How many entries one node holds before it splits.
///
/// Eight keeps a node's rectangle test loop inside a cache line's worth of work and keeps the tree
/// shallow for the thousands of fragments a real document has.
pub(crate) const MAX_ENTRIES: usize = 8;

/// The fewest entries a node keeps after a split, which is what stops a split producing a node
/// holding one entry and a node holding the rest.
pub(crate) const MIN_ENTRIES: usize = 3;

/// A spatial index over the fragment rectangles of one coordinate system.
///
/// One tree per space, because a rectangle is only a rectangle in the space it was measured in: a
/// hierarchy holding two spaces' rectangles would have to express both on the device to compare
/// them, and expressing them on the device is what stops being true when a space moves.
#[derive(Debug, Default)]
pub(crate) struct RTree {
    /// The coordinate system every rectangle in this tree is in, written into each entry's home so
    /// that a fragment name alone is enough to find the tree holding it.
    space: Option<SpatialId>,
    /// Every node. Nodes are never moved, so an index into this is a stable name.
    nodes: Vec<Node>,
    /// The root node, if the tree holds anything.
    root: Option<usize>,
    /// How many entries are held.
    len: usize,
    /// How the entries this tree has been told about were placed.
    placements: Placements,
}

/// How many placements each of the three paths took.
///
/// Kept on the tree rather than read from the process-wide counters because it is what a test
/// asserts on, and a shared counter cannot say which tree moved anything.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Placements {
    /// Entries rewritten inside the leaf already holding them, touching nothing else.
    pub(crate) in_place: u64,
    /// Entries rewritten in their leaf, with the envelopes above brought up to date.
    pub(crate) stretched: u64,
    /// Entries taken out of the hierarchy and put back somewhere else.
    pub(crate) reinserted: u64,
}

impl RTree {
    /// An empty tree over one coordinate system.
    pub(crate) fn for_space(space: Option<SpatialId>) -> Self {
        Self {
            space,
            ..Self::default()
        }
    }

    /// How many entries are held.
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// Forgets every node, which a caller only reaches with nothing left to forget.
    ///
    /// The homes are not touched: they belong to the forest, and this tree's entries have each
    /// given theirs up on the way out.
    fn reset(&mut self) {
        self.nodes.clear();
        self.root = None;
        self.len = 0;
    }

    /// Adds one entry.
    pub(crate) fn insert(
        &mut self,
        key: FragKey,
        bounds: Rect<DevicePx, Device>,
        homes: &mut Homes,
    ) {
        self.len += 1;
        let Some(root) = self.root else {
            let root = self.push(Node::leaf(key, bounds));
            self.root = Some(root);
            self.file(key, root, homes);
            return;
        };
        if let Some(sibling) = insert::into(self, root, key, bounds, homes) {
            // The root split, so the tree grows a level: a new root over the two halves.
            let envelope = self.nodes[root]
                .envelope
                .union(self.nodes[sibling].envelope);
            let grown = self.push(Node::internal(vec![root, sibling], envelope));
            self.nodes[root].parent = Some(grown);
            self.nodes[sibling].parent = Some(grown);
            self.root = Some(grown);
        }
    }

    /// Removes one entry by name, and reports whether it was there.
    pub(crate) fn remove(&mut self, key: FragKey, homes: &mut Homes) -> bool {
        remove::take(self, key, homes)
    }

    /// The leaf of *this* tree holding one entry, if this tree is the one holding it.
    ///
    /// A home names a tree as well as a leaf, and the check is not a formality: the forest moves an
    /// entry between trees when its box changes coordinate system, and a leaf index read from the
    /// tree that used to hold it names some unrelated node in this one.
    fn leaf_of(&self, key: FragKey, homes: &Homes) -> Option<usize> {
        let home = homes.get(key)?;
        (home.space == self.space).then_some(home.leaf)
    }

    /// Records that one entry sits in one of this tree's leaves.
    fn file(&self, key: FragKey, leaf: usize, homes: &mut Homes) {
        homes.insert(
            key,
            Home {
                space: self.space,
                leaf,
            },
        );
    }

    /// Records that one entry now covers `bounds`, wherever it happens to be.
    ///
    /// See [`place::at`] for which of the three answers is taken and why.
    pub(crate) fn place(
        &mut self,
        key: FragKey,
        bounds: Rect<DevicePx, Device>,
        homes: &mut Homes,
    ) -> Placed {
        let placed = place::at(self, key, bounds, homes);
        match placed {
            Placed::InPlace => {
                self.placements.in_place += 1;
                counter::bump(Counter::HitEntriesMovedInPlace);
            }
            Placed::Stretched => {
                self.placements.stretched += 1;
                counter::bump(Counter::HitEntriesMovedInPlace);
            }
            Placed::Reinserted => {
                self.placements.reinserted += 1;
                counter::bump(Counter::HitEntriesReinserted);
            }
        }
        placed
    }

    /// Records that a whole run of entries now covers the rectangles given, in one pass.
    ///
    /// See the [`settle`](mod@settle) module for why a run is not the same call repeated.
    pub(crate) fn settle(&mut self, moved: &[Carried], homes: &mut Homes) {
        settle::settle(self, moved, homes);
    }

    /// Every entry whose rectangle contains `point`, in no particular order.
    pub(crate) fn query(&self, point: Point<DevicePx, Device>, out: &mut impl Extend<FragKey>) {
        let Some(root) = self.root else {
            return;
        };
        query::collect(self, root, point, out);
    }

    /// One node, by the index that names it.
    fn node(&self, at: usize) -> &Node {
        &self.nodes[at]
    }

    /// Adds a node and returns its index.
    fn push(&mut self, node: Node) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }
}

/// What the tests reach for, and nothing else does.
///
/// Declared below everything that ships so that the whole of the module above this line is the
/// module's shipped code, read as such by anything that goes looking for which stage moves which
/// counter.
#[cfg(test)]
mod testing {
    use super::{Placements, RTree};

    impl Placements {
        /// How many placements did not go back through the hierarchy.
        pub(crate) fn kept(self) -> u64 {
            self.in_place + self.stretched
        }
    }

    impl RTree {
        /// How the entries this tree holds were last placed.
        pub(crate) fn placements(&self) -> Placements {
            self.placements
        }
    }
}

#[cfg(test)]
mod tests;
