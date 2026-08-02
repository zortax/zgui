//! Which document a node belongs to, and which of that document's arenas holds it.

use zgui_arena::{ArenaKind, DomainId};

pub use zgui_arena::DocumentId;

/// Which of a document's arenas holds its nodes.
///
/// A document keeps several arenas — nodes here, boxes and fragments elsewhere — and they index
/// independently, so the same slot number occurs in all of them at once. Naming the node arena
/// once, here, is what keeps a key to a node from ever resolving inside a key space it does not
/// belong to.
pub const NODE_ARENA: ArenaKind = match ArenaKind::new(0) {
    Some(kind) => kind,
    None => panic!("zero is always a valid arena kind"),
};

/// The domain a document's node keys carry.
pub const fn node_domain(document: DocumentId) -> DomainId {
    DomainId::new(document, NODE_ARENA)
}

#[cfg(test)]
mod tests {
    use zgui_arena::DocumentId;

    use super::{NODE_ARENA, node_domain};

    #[test]
    fn two_documents_have_disjoint_node_domains() {
        let first = node_domain(DocumentId::FIRST);
        let second = node_domain(DocumentId::new(1).expect("in range"));
        assert_ne!(first, second);
        assert_eq!(first.arena(), NODE_ARENA);
        assert_eq!(second.document(), DocumentId::new(1).expect("in range"));
    }
}
