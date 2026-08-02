//! The tree selector matching walks, which is not the tree the cascade walks.
//!
//! `+`, `~`, `:first-child` and `:nth-child` all step along the element-only chain, on the hot path,
//! once per candidate. Deriving that chain from the plain one at match time turns a constant-time
//! step into a scan past however much text happens to be in the way, so both chains are maintained
//! when a node is linked in and every step here is a single link.
//!
//! Everything in this module is reachable on a node of any kind and correct there: the links it
//! follows already skip text and markers.

use core::ptr::NonNull;

use selectors::OpaqueElement;

use crate::id::opaque::opaque_element;
use crate::node::handle::Node;

impl<'doc> Node<'doc> {
    /// This element's identity, as the matcher keys it.
    ///
    /// A real pointer to the record, not a disguised index. Record addresses are fixed for the life
    /// of the document, so the pointer is a stable identity — and being a real pointer is what would
    /// let a scoped rule turn an opaque element back into an element rather than merely comparing
    /// two integers.
    pub fn opaque_identity(self) -> OpaqueElement {
        let record: NonNull<()> = opaque_element(self.record()).cast();
        OpaqueElement::from_non_null_ptr(record)
    }

    /// The nearest element ancestor, which is what a descendant combinator steps to.
    pub fn parent_element_handle(self) -> Option<Node<'doc>> {
        self.parent_node_handle()
            .filter(|parent| parent.kind().is_element())
    }
}
