//! Recording what a selector needed to know about an element.
//!
//! When the matcher answers a question that a later mutation could change — "is this the last
//! element child", "is this the only one", "which position is it in" — it records a flag so that the
//! mutation knows to invalidate. This is the one method in the whole matching surface that
//! **writes**, and it writes in two places: on the element being matched *and on that element's
//! parent*, because "am I the last child" is a fact about the parent's child list.
//!
//! That is why the field behind it is an atomic and not a cell. Two workers matching two siblings
//! write the same parent's word at the same moment, from two threads, with nothing between them —
//! and a read-modify-write of a plain integer there would lose one of the two flags, which would
//! show up much later as a rounded corner that stops updating.

use selectors::matching::ElementSelectorFlags;

use crate::node::handle::Node;

impl Node<'_> {
    /// Records `flags`, splitting them between this element and its parent as the matcher intends.
    pub fn record_selector_flags(self, flags: ElementSelectorFlags) {
        let for_self = flags.for_self();
        if !for_self.is_empty() {
            self.record().insert_selector_flags(for_self);
        }
        let for_parent = flags.for_parent();
        if !for_parent.is_empty()
            && let Some(parent) = self.parent_node_handle()
        {
            parent.record().insert_selector_flags(for_parent);
        }
    }
}

#[cfg(test)]
mod tests {
    use selectors::matching::ElementSelectorFlags;
    use zgui_interned::ElementName;

    use crate::arena::document::Document;
    use crate::node::kind::NodeKind;

    #[test]
    fn a_flag_meant_for_the_parent_lands_on_the_parent_and_not_on_the_child() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let child = document.append(root, NodeKind::Element, ElementName::new("item"));

        document
            .node(child)
            .record_selector_flags(ElementSelectorFlags::HAS_SLOW_SELECTOR);
        assert!(
            document
                .store()
                .core(root)
                .selector_flags()
                .contains(ElementSelectorFlags::HAS_SLOW_SELECTOR)
        );
        assert!(document.store().core(child).selector_flags().is_empty());
    }

    #[test]
    fn a_flag_meant_for_the_element_itself_stays_there() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        document
            .node(root)
            .record_selector_flags(ElementSelectorFlags::ANCHORS_RELATIVE_SELECTOR);
        assert!(
            document
                .store()
                .core(root)
                .selector_flags()
                .contains(ElementSelectorFlags::ANCHORS_RELATIVE_SELECTOR)
        );
    }
}
