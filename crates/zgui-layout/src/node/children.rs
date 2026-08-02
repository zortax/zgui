//! Walking a box's children in layout order.
//!
//! The layout algorithms ask for this iterator inside their innermost loops, so it allocates
//! nothing and filters nothing: every decision about which boxes a container has, and in what
//! order, was already taken when the box tree was built.

use taffy::NodeId;
use zgui_dom::side::BoxKey;

use crate::key::to_node_id;

/// A box's children in layout order, as the layout engine's own identifiers.
#[derive(Clone, Debug)]
pub struct ChildIter<'a> {
    /// What is left to yield.
    keys: core::slice::Iter<'a, BoxKey>,
}

impl<'a> ChildIter<'a> {
    /// Walks the given child list.
    pub fn new(children: &'a [BoxKey]) -> Self {
        Self {
            keys: children.iter(),
        }
    }
}

impl Iterator for ChildIter<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        self.keys.next().copied().map(to_node_id)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.keys.size_hint()
    }
}

impl ExactSizeIterator for ChildIter<'_> {}

#[cfg(test)]
mod tests {
    use zgui_arena::{DomainId, Generation};
    use zgui_dom::side::BoxKey;

    use crate::key::{from_node_id, to_node_id};

    use super::ChildIter;

    #[test]
    fn the_iterator_yields_the_list_in_order_and_nothing_else() {
        let keys: Vec<BoxKey> = (1..4)
            .map(|index| BoxKey::new(index, Generation::FIRST, DomainId::FIRST))
            .collect();
        let seen: Vec<BoxKey> = ChildIter::new(&keys).map(from_node_id).collect();
        assert_eq!(seen, keys);
        assert_eq!(ChildIter::new(&keys).len(), 3);
        assert_eq!(ChildIter::new(&[]).count(), 0);
    }

    #[test]
    fn a_key_survives_the_round_trip_through_the_engine_identifier() {
        let key = BoxKey::new(9, Generation::FIRST, DomainId::FIRST);
        assert_eq!(from_node_id(to_node_id(key)), key);
    }
}
