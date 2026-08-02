//! A backend that reaches past the batch API and writes the arena itself.

/// Adds a class without recording anything, marking anything, or asking for a frame.
pub fn add_class(document: &mut Document, node: NodeIndex, class: ClassName) {
    document.store_mut().write_classes(node, &[class]);
}
