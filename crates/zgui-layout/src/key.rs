//! What a box is called, and the two other places that name has to be understood.
//!
//! A box has exactly one name — [`BoxKey`], the generation-checked handle the document already
//! records against every element that generated boxes. This crate does not mint a second one.
//!
//! # Why the document names a box it does not own
//!
//! The record for a box, its formatting context and its two child lists all belong here, and the
//! document must not depend on this crate. But the document still has to answer "which boxes did
//! this element generate", because accessibility geometry, hit results and imperative queries all
//! ask it. So the *name* is declared there, over a type that has no values, and the *record* is
//! declared here. One name, two declarations of what it names, and the type system keeps a box
//! name from ever being passed where an element name is expected.
//!
//! Two consequences are handled below and nowhere else. The arena that stores box records hands
//! out handles naming its own record type, so two total functions here move a handle between the
//! two spellings of what it names, and neither touches the bits. And the layout engine
//! wants an opaque 64-bit integer, so [`to_node_id`] and [`from_node_id`] pack and unpack one.

use taffy::NodeId;
use zgui_arena::Key;
use zgui_dom::side::BoxKey;

use crate::node::box_node::BoxNode;

/// The document's name for a box the arena just handed us.
///
/// Index, generation and arena identity are carried across unchanged, so this is a change of what
/// the handle is *declared* to name and not of which slot it resolves to.
pub(crate) fn named(key: Key<BoxNode>) -> BoxKey {
    BoxKey::new(key.index(), key.generation(), key.domain())
}

/// The arena's own name for a box the document named.
///
/// The exact inverse of [`named`].
pub(crate) fn slot(key: BoxKey) -> Key<BoxNode> {
    Key::new(key.index(), key.generation(), key.domain())
}

/// A box's name as the layout engine's opaque identifier.
///
/// The engine never interprets the bits; it only hands them back.
pub fn to_node_id(key: BoxKey) -> NodeId {
    NodeId::new(key.as_u64())
}

/// The box an identifier the layout engine handed back names.
///
/// # Panics
///
/// If the identifier did not come from [`to_node_id`]. Every identifier the engine sees was
/// produced there, so this cannot happen for an engine-supplied one; it is a guard against an
/// identifier invented elsewhere, which would otherwise resolve to an unrelated slot.
pub fn from_node_id(id: NodeId) -> BoxKey {
    try_from_node_id(id).expect("every node identifier the layout engine sees is a box key")
}

/// The box an identifier names, or nothing if those bits are not a box name.
pub fn try_from_node_id(id: NodeId) -> Option<BoxKey> {
    BoxKey::from_u64(u64::from(id))
}

#[cfg(test)]
mod tests {
    use taffy::NodeId;
    use zgui_arena::{ArenaKind, DocumentId, DomainId, Generation, Key};
    use zgui_dom::side::BoxKey;

    use crate::node::box_node::BoxNode;

    use super::{from_node_id, named, slot, to_node_id, try_from_node_id};

    /// A domain that is not the first, so a bridge that dropped the arena identity would show.
    fn domain() -> DomainId {
        DomainId::new(
            DocumentId::new(3).expect("in range"),
            ArenaKind::new(2).expect("in range"),
        )
    }

    #[test]
    fn renaming_a_handle_changes_nothing_about_which_slot_it_names() {
        let generation = Generation::FIRST.next().expect("a second occupant");
        let arena_key: Key<BoxNode> = Key::new(41, generation, domain());
        let document_key = named(arena_key);
        assert_eq!(document_key.index(), 41);
        assert_eq!(document_key.generation(), generation);
        assert_eq!(document_key.domain(), domain());
        assert_eq!(document_key.as_u64(), arena_key.as_u64());
        assert_eq!(slot(document_key), arena_key);
    }

    #[test]
    fn a_box_name_survives_the_engine_identifier() {
        let key = BoxKey::new(7, Generation::FIRST, domain());
        assert_eq!(from_node_id(to_node_id(key)), key);
    }

    #[test]
    fn bits_no_handle_can_have_are_rejected_rather_than_resolved() {
        // A zero generation is the never-issued-or-retired sentinel, so these bits name no slot.
        let retired = u64::from(domain().as_u16()) << 48 | 7;
        assert_eq!(try_from_node_id(NodeId::new(retired)), None);
        assert_eq!(try_from_node_id(NodeId::new(0)), None);
    }
}
