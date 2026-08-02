//! The two bare identities other subsystems key their own tables by.
//!
//! Neither is a new name. Both are an existing one squeezed into the shape a foreign interface
//! insists on, and the point of collecting them here is that the squeeze happens once, with the
//! reason written down, rather than at each of the places that needs it.
//!
//! **A node's opaque identity is its slot number.** The style engine keys its snapshot map by it,
//! and a snapshot is taken and consumed inside one frame — within which a slot cannot come to mean
//! something else, because freed slots are held back until the frame ends. A generation would cost
//! a widening on every lookup and rule out nothing that is reachable.
//!
//! **An element's opaque identity is the address of its record.** Addresses are stable for the
//! life of the document, so a pointer is a legitimate identity here, and it is the one shape that
//! keeps the engine's scope resolution — which compares an element against a pointer it was handed
//! earlier — able to answer at all.

use core::ptr::NonNull;

use crate::id::node_key::NodeIndex;
use crate::node::inner::NodeInner;

/// The bare integer identity of a node.
///
/// Round-trips with [`NodeIndex`] and carries no generation, for the reason in the module
/// documentation.
pub const fn opaque_node(node: NodeIndex) -> usize {
    node.get() as usize
}

/// The node an opaque integer identity names.
pub const fn node_from_opaque(opaque: usize) -> NodeIndex {
    NodeIndex::new(opaque as u32)
}

/// The bare pointer identity of an element.
///
/// Two handles to the same node produce the same pointer, and no two live nodes produce the same
/// one, because a record's address is fixed for as long as the document holds it.
pub fn opaque_element(record: &NodeInner) -> NonNull<NodeInner> {
    NonNull::from(record)
}

#[cfg(test)]
mod tests {
    use super::{node_from_opaque, opaque_node};
    use crate::id::node_key::NodeIndex;

    #[test]
    fn the_integer_identity_round_trips() {
        for index in [0, 1, 4096, u32::MAX - 1] {
            let node = NodeIndex::new(index);
            assert_eq!(node_from_opaque(opaque_node(node)), node);
        }
    }
}
