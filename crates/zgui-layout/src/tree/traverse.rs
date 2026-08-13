//! Walking the box tree downwards, which is all the layout algorithms ever do.
//!
//! There is no upward step here and none is needed: everything the algorithms want from an ancestor
//! is passed down as an argument. Everything *we* want from an ancestor — invalidation, the
//! containing block an out-of-flow box is positioned against — is maintained on the box itself.

use taffy::{NodeId, TraversePartialTree, TraverseTree};

use crate::key::from_node_id;
use crate::node::children::ChildIter;
use crate::tree::LayoutTree;

impl<C> TraversePartialTree for LayoutTree<'_, C> {
    type ChildIter<'a>
        = ChildIter<'a>
    where
        Self: 'a;

    fn child_ids(&self, parent: NodeId) -> Self::ChildIter<'_> {
        ChildIter::new(&self.structure().node(from_node_id(parent)).children)
    }

    fn child_count(&self, parent: NodeId) -> usize {
        self.structure().node(from_node_id(parent)).children.len()
    }

    fn get_child_id(&self, parent: NodeId, index: usize) -> NodeId {
        let children = &self.structure().node(from_node_id(parent)).children;
        crate::key::to_node_id(children[index])
    }
}

impl<C> TraverseTree for LayoutTree<'_, C> {}
