//! Slot numbers, optional slot numbers, and the generation-checked key they widen into.

use zgui_arena::Key;

use crate::node::inner::NodeInner;

/// A generation-checked handle to one node of one document.
///
/// This is the name a node carries across a boundary: into a side table, into another subsystem's
/// map, into a value that outlives the frame it was taken in. A key from a removed node never
/// resolves to whatever moved into its slot afterwards, and a key from one document never
/// resolves inside another.
///
/// Inside a single document's own walks, [`NodeIndex`] is used instead: the generation cannot have
/// changed under a walk that never leaves the document, and checking it anyway would cost a load
/// per step for no new information.
pub type NodeKey = Key<NodeInner>;

crate::plain_data!(NodeKey);

/// A slot number inside one document's node arena: a [`NodeKey`] with its generation and arena
/// identity stripped.
///
/// Links, walks and dirty-tracking all travel in this space. It is four bytes rather than eight,
/// which is what lets a record hold eight links plus its ordinals in a single cache line.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
pub struct NodeIndex(u32);

impl NodeIndex {
    /// The slot number `index`.
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The slot number as a plain integer.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// [`Option<NodeIndex>`] in four bytes: [`u32::MAX`] is the absent case.
///
/// Four billion slots is already past the point where the arena itself gives up, so spending the
/// top value on the absent case costs nothing that was reachable.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[repr(transparent)]
pub struct OptIndex(u32);

impl OptIndex {
    /// The absent index.
    pub const NONE: Self = Self(u32::MAX);

    /// The index `node`.
    pub const fn some(node: NodeIndex) -> Self {
        Self(node.0)
    }

    /// The index this names, if it names one.
    pub const fn get(self) -> Option<NodeIndex> {
        if self.0 == u32::MAX {
            None
        } else {
            Some(NodeIndex(self.0))
        }
    }

    /// Whether this names no index at all.
    pub const fn is_none(self) -> bool {
        self.0 == u32::MAX
    }

    /// The optional form of an [`Option<NodeIndex>`].
    pub const fn from_option(node: Option<NodeIndex>) -> Self {
        match node {
            Some(node) => Self::some(node),
            None => Self::NONE,
        }
    }
}

impl Default for OptIndex {
    fn default() -> Self {
        Self::NONE
    }
}

impl From<NodeIndex> for OptIndex {
    fn from(node: NodeIndex) -> Self {
        Self::some(node)
    }
}

#[cfg(test)]
mod tests {
    use super::{NodeIndex, OptIndex};

    #[test]
    fn an_optional_index_is_four_bytes_and_round_trips() {
        assert_eq!(size_of::<OptIndex>(), 4);
        assert_eq!(OptIndex::NONE.get(), None);
        assert!(OptIndex::NONE.is_none());
        for index in [0, 1, 7, u32::MAX - 1] {
            let node = NodeIndex::new(index);
            assert_eq!(OptIndex::some(node).get(), Some(node));
            assert!(!OptIndex::some(node).is_none());
        }
    }

    #[test]
    fn the_optional_form_of_an_option_agrees_with_it() {
        assert_eq!(OptIndex::from_option(None), OptIndex::NONE);
        let node = NodeIndex::new(3);
        assert_eq!(OptIndex::from_option(Some(node)).get(), Some(node));
    }
}
