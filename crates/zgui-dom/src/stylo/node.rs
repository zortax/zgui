//! The node trait: what kind of node this is, its identity, and the plain tree links.
//!
//! These are the links the *style traversal* walks, and they are not the ones selector matching
//! walks: a text node between two elements appears here and does not appear on the element-only
//! chain. Keeping the two apart is what makes `:nth-child` and `+` count elements rather than
//! nodes.

use style::dom::{NodeInfo, OpaqueNode, TNode};

use crate::id::node_key::NodeIndex;
use crate::id::opaque::opaque_node;
use crate::node::handle::Node;
use crate::node::kind::NodeKind;

impl NodeInfo for Node<'_> {
    fn is_element(&self) -> bool {
        self.kind() == NodeKind::Element
    }

    fn is_text_node(&self) -> bool {
        self.kind() == NodeKind::Text
    }
}

impl<'doc> TNode for Node<'doc> {
    type ConcreteElement = Node<'doc>;
    type ConcreteDocument = Node<'doc>;
    type ConcreteShadowRoot = Node<'doc>;

    fn parent_node(&self) -> Option<Self> {
        self.record().parent().map(|index| self.sibling(index))
    }

    fn first_child(&self) -> Option<Self> {
        self.record().first_child().map(|index| self.sibling(index))
    }

    fn last_child(&self) -> Option<Self> {
        self.record().last_child().map(|index| self.sibling(index))
    }

    fn prev_sibling(&self) -> Option<Self> {
        self.record()
            .prev_sibling()
            .map(|index| self.sibling(index))
    }

    fn next_sibling(&self) -> Option<Self> {
        self.record()
            .next_sibling()
            .map(|index| self.sibling(index))
    }

    fn owner_doc(&self) -> Self::ConcreteDocument {
        self.sibling(NodeIndex::new(0))
    }

    /// Whether this node is attached to the document rather than held in a detached fragment.
    fn is_in_document(&self) -> bool {
        self.record()
            .has_flags(crate::node::flags::NodeFlags::IN_DOCUMENT)
    }

    /// The nearest ancestor the traversal descends from, which is the nearest element ancestor.
    fn traversal_parent(&self) -> Option<Self::ConcreteElement> {
        self.parent_node().and_then(|node| node.as_element())
    }

    /// This node's bare integer identity: its slot number.
    ///
    /// The engine keys its snapshot map by this, and a snapshot is taken and consumed inside one
    /// frame — within which a slot cannot come to mean something else, because a freed slot is held
    /// back until the frame ends.
    fn opaque(&self) -> OpaqueNode {
        OpaqueNode(opaque_node(self.index()))
    }

    fn debug_id(self) -> usize {
        opaque_node(self.index())
    }

    fn as_element(&self) -> Option<Self::ConcreteElement> {
        self.kind().is_element().then_some(*self)
    }

    fn as_document(&self) -> Option<Self::ConcreteDocument> {
        (self.kind() == NodeKind::Document).then_some(*self)
    }

    /// Never a shadow root: this document has no shadow trees.
    fn as_shadow_root(&self) -> Option<Self::ConcreteShadowRoot> {
        None
    }
}
