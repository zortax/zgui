//! A backend that changes the document only through its batch API.

/// Adds a class, the only way there is.
pub fn add_class(document: &Document, node: NodeIndex, class: ClassName) {
    document
        .edit(&EverythingMatters, |edit| edit.add_class(node, class))
        .expect("not poisoned");
}
