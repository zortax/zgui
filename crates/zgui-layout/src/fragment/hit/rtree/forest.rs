//! One hierarchy per coordinate system, and the table that says which one holds what.
//!
//! # Why the index is not one hierarchy
//!
//! A bounding-volume hierarchy compares rectangles, and two rectangles are only comparable in one
//! space. A single hierarchy over a whole document therefore has to hold every rectangle on the
//! device — which means every entry under an animating box has to be rewritten on every tick of
//! that animation, by whatever pass happens to be walking, or the hierarchy answers for where the
//! box used to be.
//!
//! Grouped by space instead, the tick rewrites nothing at all: the matrix moved, and a matrix is a
//! property of the space rather than of the entries in it. What a query pays for that is one
//! inverse per *space* — a handful, and fewer than the one-per-candidate the other arrangement
//! pays anyway.

use zgui_geom::{Device, DevicePx, Rect};
use zgui_scene::SpatialId;

use crate::fragment::FragKey;
use crate::fragment::hit::rtree::home::Homes;
use crate::fragment::hit::rtree::{Placed, RTree};

/// One entry of a run that moved together, waiting to be settled.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Carried {
    /// The fragment that moved.
    pub(crate) frag: FragKey,
    /// The coordinate system it moved within, which decides the tree that has to hear about it.
    pub(crate) space: Option<SpatialId>,
    /// Where it is now, in that space.
    pub(crate) bounds: Rect<DevicePx, Device>,
}

/// Every coordinate system's hierarchy, and every entry's home.
#[derive(Debug, Default)]
pub(crate) struct Forest {
    /// One hierarchy per coordinate system anything is indexed in.
    ///
    /// A list and not a map, because what this is *for* is being walked from end to end: a query
    /// visits every hierarchy, and a document holds a couple of dozen. Finding one by name happens
    /// only when an entry is written, is a scan of the same couple of dozen, and is dwarfed by the
    /// tree descent that follows it.
    trees: Vec<(Option<SpatialId>, RTree)>,
    /// Which tree and which leaf each entry is in.
    homes: Homes,
    /// How many entries are held, across every tree.
    len: usize,
}

impl Forest {
    /// How many entries are held.
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// How many coordinate systems hold anything.
    pub(crate) fn spaces(&self) -> usize {
        self.trees.len()
    }

    /// Every hierarchy, with the coordinate system its rectangles are in.
    ///
    /// In no particular order, which a caller answering a query has to survive: a hit is decided by
    /// painting order and by nothing about how the entries were stored.
    pub(crate) fn trees(&self) -> impl Iterator<Item = (Option<SpatialId>, &RTree)> {
        self.trees.iter().map(|(space, tree)| (*space, tree))
    }

    /// The hierarchy for one coordinate system, established if this is the first entry in it.
    fn tree_of(
        trees: &mut Vec<(Option<SpatialId>, RTree)>,
        space: Option<SpatialId>,
    ) -> &mut RTree {
        let at = match trees.iter().position(|(held, _)| *held == space) {
            Some(at) => at,
            None => {
                trees.push((space, RTree::for_space(space)));
                trees.len() - 1
            }
        };
        &mut trees[at].1
    }

    /// Forgets everything.
    pub(crate) fn clear(&mut self) {
        self.trees.clear();
        self.homes.clear();
        self.len = 0;
    }

    /// Adds one entry to the tree for its space.
    pub(crate) fn insert(
        &mut self,
        frag: FragKey,
        space: Option<SpatialId>,
        bounds: Rect<DevicePx, Device>,
    ) {
        let Self { trees, homes, len } = self;
        Self::tree_of(trees, space).insert(frag, bounds, homes);
        *len += 1;
    }

    /// Records where one entry now is, moving it between trees if its space changed.
    ///
    /// A box that has changed coordinate system has not moved within the one it was in — it has
    /// left it — so the rectangle it is being placed at means nothing to the tree still holding it.
    /// Taking it out first is what stops that tree answering for a rectangle it can no longer
    /// express.
    pub(crate) fn place(
        &mut self,
        frag: FragKey,
        space: Option<SpatialId>,
        bounds: Rect<DevicePx, Device>,
    ) -> Placed {
        if self.homes.get(frag).is_some_and(|home| home.space != space) {
            self.remove(frag);
        }
        let Self { trees, homes, len } = self;
        let tree = Self::tree_of(trees, space);
        let held = tree.len();
        let placed = tree.place(frag, bounds, homes);
        *len += tree.len() - held;
        placed
    }

    /// Takes one entry out of whichever tree holds it, and reports whether one did.
    pub(crate) fn remove(&mut self, frag: FragKey) -> bool {
        let Some(space) = self.homes.get(frag).map(|home| home.space) else {
            return false;
        };
        let Self { trees, homes, len } = self;
        let Some(at) = trees.iter().position(|(held, _)| *held == space) else {
            // A home naming a tree that is not there is a fact about nothing. The name goes, or
            // every later lookup for this fragment reads it again.
            homes.remove(frag);
            return false;
        };
        let taken = trees[at].1.remove(frag, homes);
        if taken {
            *len -= 1;
        }
        // An empty tree is a coordinate system nothing is drawn in any more, and a query walks
        // every one it holds.
        if trees[at].1.len() == 0 {
            trees.swap_remove(at);
        }
        taken
    }

    /// Ends a run of carried entries, one tree at a time.
    ///
    /// The run is walked in contiguous stretches of one space rather than sorted, because a run is
    /// produced by a walk over a rigidly moving subtree and a subtree that moves rigidly is a
    /// subtree in which nothing establishes a coordinate system of its own: in practice the whole
    /// run is one stretch, and a run that is not is still settled correctly, one stretch at a time.
    pub(crate) fn settle(&mut self, carried: &[Carried]) {
        let Self { trees, homes, len } = self;
        let mut rest = carried;
        while let Some(first) = rest.first() {
            let space = first.space;
            let end = rest
                .iter()
                .position(|entry| entry.space != space)
                .unwrap_or(rest.len());
            let (run, remaining) = rest.split_at(end);
            let tree = Self::tree_of(trees, space);
            let held = tree.len();
            tree.settle(run, homes);
            *len += tree.len() - held;
            rest = remaining;
        }
    }
}

#[cfg(test)]
mod tests {
    use zgui_arena::{ArenaKind, DocumentId, DomainId, Generation, Key};
    use zgui_geom::Matrix4;
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};
    use zgui_scene::{OwnSpace, PropertyOwner, SpatialTree};

    use super::Forest;
    use crate::fragment::FragKey;

    /// A fragment name for one slot number.
    fn key(index: u32) -> FragKey {
        Key::new(
            index,
            Generation::FIRST,
            DomainId::new(DocumentId::FIRST, ArenaKind::new(2).expect("a valid arena")),
        )
    }

    /// A rectangle from four numbers.
    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(x), DevicePx(y)),
            Size::new(DevicePx(width), DevicePx(height)),
        )
    }

    /// Two coordinate systems under the viewport.
    fn two_spaces() -> SpatialTree {
        let mut tree = SpatialTree::with_viewport();
        for owner in [2, 3] {
            let owner = PropertyOwner::new(owner).expect("a handle is never the empty word");
            let own = OwnSpace::of(Some(Matrix4::translation(0.0, 0.0, 0.0)), None, false);
            tree.space_of(tree.viewport(), owner, own);
        }
        tree
    }

    #[test]
    fn a_fragment_that_changes_space_stops_answering_in_the_one_it_left() {
        // The case a single hierarchy never had: a box whose transform appears — or whose sticky
        // ancestor changes — moves between trees, and the rectangle it is being placed at means
        // nothing to the tree it came from. Left there, that tree answers for a rectangle it can no
        // longer express, in a space that is about to be resolved with somebody else's matrix.
        let spatial = two_spaces();
        let mut spaces = spatial.ids();
        let (first, second) = (
            Some(spaces.next().expect("two spaces")),
            Some(spaces.next().expect("two spaces")),
        );
        let mut forest = Forest::default();
        forest.insert(key(1), first, rect(0.0, 0.0, 10.0, 10.0));
        forest.place(key(1), second, rect(0.0, 0.0, 10.0, 10.0));

        assert_eq!(
            forest.len(),
            1,
            "one entry, however many trees it has been in"
        );
        assert_eq!(forest.spaces(), 1, "and the space it left holds nothing");
        let mut found = Vec::new();
        for (space, tree) in forest.trees() {
            assert_eq!(space, second);
            tree.query(Point::new(DevicePx(5.0), DevicePx(5.0)), &mut found);
        }
        assert_eq!(found, vec![key(1)]);
    }

    #[test]
    fn a_space_nothing_is_drawn_in_costs_a_query_nothing() {
        // A query pays one inverse per tree, so a coordinate system whose last entry has gone has to
        // stop being one — otherwise every space a document has ever had is walked on every pointer
        // move for the rest of the session.
        let spatial = two_spaces();
        let mut spaces = spatial.ids();
        let (first, second) = (
            Some(spaces.next().expect("two spaces")),
            Some(spaces.next().expect("two spaces")),
        );
        let mut forest = Forest::default();
        forest.insert(key(1), first, rect(0.0, 0.0, 10.0, 10.0));
        forest.insert(key(2), second, rect(0.0, 0.0, 10.0, 10.0));
        assert_eq!(forest.spaces(), 2);

        assert!(forest.remove(key(1)));
        assert_eq!(forest.spaces(), 1);
        assert_eq!(forest.len(), 1);
        assert!(
            !forest.remove(key(1)),
            "it is not there to be removed twice"
        );
    }
}
