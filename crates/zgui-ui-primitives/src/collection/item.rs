//! One registered item, and the name it is known by.

use zgui::prelude::{GetUntracked, NodeRef, Signal};
use zgui::reactive::LocalStorage;

/// One item's name inside its own [`Collection`](crate::Collection).
///
/// Opaque and unique within the collection that minted it. It is not an index: an index moves when
/// anything before it comes or goes, and the whole point of a collection is that items do.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
pub struct ItemId(u64);

impl ItemId {
    /// Wraps a collection's own number.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The number.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// One item of a [`Collection`](crate::Collection): what it is called, and where it is.
#[derive(Copy, Clone)]
pub struct CollectionItem {
    /// What it is called.
    id: ItemId,
    /// The element it registered.
    node: NodeRef,
    /// Whether the keyboard may land on it.
    reachable: Signal<bool, LocalStorage>,
}

impl CollectionItem {
    /// Pairs a name with an element the keyboard may always land on.
    pub fn new(id: ItemId, node: NodeRef) -> Self {
        Self::reachable_when(id, node, Signal::stored_local(true))
    }

    /// The same, for an item the keyboard should skip while `reachable` reads false.
    ///
    /// A disabled tab is the case: arrowing onto it would leave the focus ring on a control that
    /// refuses to be chosen, and — in a strip that shows the panel it lands on — the shown panel
    /// and the focused tab would name different things.
    pub fn reachable_when(
        id: ItemId,
        node: NodeRef,
        reachable: Signal<bool, LocalStorage>,
    ) -> Self {
        Self {
            id,
            node,
            reachable,
        }
    }

    /// What this item is called.
    pub fn id(&self) -> ItemId {
        self.id
    }

    /// The element it registered.
    pub fn node(&self) -> NodeRef {
        self.node
    }

    /// Whether the keyboard may land on it now.
    pub fn is_reachable(&self) -> bool {
        self.reachable.get_untracked()
    }

    /// Moves focus to this item.
    pub fn focus(&self) {
        self.node.focus();
    }
}

impl core::fmt::Debug for CollectionItem {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("CollectionItem")
            .field("id", &self.id)
            .field("node", &self.node.get_untracked())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::ItemId;

    #[test]
    fn an_item_name_is_not_an_index() {
        // Stated because the mistake it prevents is invisible: an index into a list that items
        // leave silently addresses a different item afterwards.
        let first = ItemId::new(1);
        let second = ItemId::new(2);
        assert_ne!(first, second);
        assert_eq!(first.get(), 1);
    }
}
