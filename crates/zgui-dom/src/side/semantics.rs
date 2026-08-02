//! What an element means, as opposed to what it looks like.
//!
//! Semantics are written by whatever builds the document and read by the accessibility projection.
//! They are boxed and sparse because a document is mostly containers: the handful of nodes that are
//! a button, a checkbox or a labelled region carry a record, and the rest carry a null pointer's
//! worth of nothing.
//!
//! The record itself is defined in the shared vocabulary rather than here, because a view writes it
//! and a projection reads it and neither may name the other.

use zgui_vocab::Semantics;

/// One node's semantics, boxed so the column costs a pointer for the nodes that have none.
pub type SemanticsSlot = Option<Box<Semantics>>;

#[cfg(test)]
mod tests {
    use zgui_vocab::{A11y, Role, Semantics};

    use super::SemanticsSlot;

    #[test]
    fn a_slot_costs_one_pointer_when_it_is_empty() {
        assert_eq!(size_of::<SemanticsSlot>(), size_of::<usize>());
        let empty: SemanticsSlot = None;
        assert!(empty.is_none());
    }

    #[test]
    fn a_filled_slot_carries_the_role_through() {
        let slot: SemanticsSlot = Some(Box::new(Semantics::from(
            A11y::new(Role::Button).label("Save"),
        )));
        assert_eq!(slot.map(|semantics| semantics.role), Some(Role::Button));
    }
}
