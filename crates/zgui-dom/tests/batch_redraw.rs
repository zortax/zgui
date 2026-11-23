//! A batch asks for a frame only when it changed the document.

use zgui_dom::{Document, EverythingMatters, NodeKind};
use zgui_interned::{ClassName, ElementName};
use zgui_vocab::UiState;

#[test]
fn a_batch_that_changes_nothing_asks_for_no_frame_and_one_that_changes_something_does() {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    document.take_redraw_request();

    document
        .edit(&EverythingMatters, |_| {})
        .expect("the document is not poisoned");
    document
        .edit(&EverythingMatters, |batch| {
            batch.watches(root, UiState::HOVER)
        })
        .expect("the document is not poisoned");
    assert!(!document.take_redraw_request());

    document
        .edit(&EverythingMatters, |batch| {
            batch.set_classes(root, &[ClassName::new("changed")]);
        })
        .expect("the document is not poisoned");
    assert!(document.take_redraw_request());
}
