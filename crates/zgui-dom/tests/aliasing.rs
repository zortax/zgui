//! The aliasing discipline the batched change API is only sound under, exercised for a checker.
//!
//! Changing a document through a shared reference means deriving an exclusive reference from a cell
//! on every call. That is sound exactly while no two such references are live at once — and the case
//! that would break it is not exotic: a listener changing the document from inside a dispatch that is
//! itself inside a batch derives a second one while the first is still on the stack.
//!
//! The rule is therefore that every change re-derives its references for the duration of one call
//! and drops them before returning, and nothing is held across anything that can open a nested
//! batch. This target is the shape that violates the rule if the rule is not kept, so a checker that
//! tracks reference provenance has something to fail on. It is deliberately small and has no style
//! engine in it, because such a checker is slow.
//!
//! Run under one with tree borrows, permissive provenance and isolation off: the atom tables the
//! document interns names into carry provenance a stricter setting rejects for reasons that are not
//! this crate's.

use zgui_bits::Dirty;
use zgui_dom::{Document, EverythingMatters, NodeKind};
use zgui_interned::{AttrName, ClassName, ElementName, Ident};
use zgui_vocab::{SharedString, UiState};

#[test]
fn a_nested_batch_derives_its_own_references_while_the_outer_ones_are_on_the_stack() {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );

    document
        .edit(&EverythingMatters, |outer| {
            let first = outer.create_element(ElementName::new("li"));
            outer.insert_before(root, first, None);

            // What a listener does: it holds the document and changes it, from inside a change.
            let held = outer.document();
            held.edit(&EverythingMatters, |inner| {
                let second = inner.create_element(ElementName::new("li"));
                inner.set_classes(second, &[ClassName::new("row")]);
                inner.insert_before(root, second, Some(first));
                inner.set_state(first, UiState::HOVER, true);
            })
            .expect("the document is not poisoned");

            outer.set_id(first, Some(Ident::new("kept")));
            outer.add_class(first, ClassName::new("row"));
        })
        .expect("the document is not poisoned");

    assert_eq!(document.store().core(root).child_count(), 2);
}

#[test]
fn every_kind_of_change_and_then_the_close_of_the_batch() {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    // The child-list flags a real matcher would have left behind, so the close of the batch has
    // something to expand rather than an empty log.
    document
        .store()
        .core(root)
        .insert_selector_flags(selectors::matching::ElementSelectorFlags::all());

    let text = document
        .edit(&EverythingMatters, |batch| {
            let first = batch.create_element(ElementName::new("li"));
            batch.insert_before(root, first, None);
            let marker = batch.create_marker();
            batch.insert_before(root, marker, None);
            let text = batch.create_text("");
            batch.insert_before(first, text, None);

            batch.set_attribute(
                first,
                AttrName::new("data-state"),
                Some(SharedString::from("open")),
            );
            batch.set_attribute(first, AttrName::new("data-state"), None);
            batch.set_observed(first, zgui_dom::side::ObservedMask::BORDER_BOX);
            batch.set_semantics(first, None);
            text
        })
        .expect("the document is not poisoned");

    document
        .edit(&EverythingMatters, |batch| batch.set_text(text, "Saved"))
        .expect("the document is not poisoned");

    let mut serviced = 0;
    let document_index = document.document_index();
    zgui_dom::dirty::walk::walk(
        document.store_mut(),
        document_index,
        Dirty::all(),
        &mut |_, _| serviced += 1,
    );
    assert!(serviced > 0);
    assert!(document.take_redraw_request());
}

#[test]
fn a_removal_and_a_move_in_one_batch() {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    let a = document.append(root, NodeKind::Element, ElementName::new("li"));
    let b = document.append(root, NodeKind::Element, ElementName::new("li"));
    let c = document.append(root, NodeKind::Element, ElementName::new("li"));

    document
        .edit(&EverythingMatters, |batch| {
            batch.remove(b);
            batch.insert_before(a, c, None);
        })
        .expect("the document is not poisoned");

    assert_eq!(document.store().core(root).child_count(), 1);
    assert_eq!(document.store().core(a).child_count(), 1);
    assert_eq!(document.take_removed(), vec![b]);
    assert_eq!(document.take_snapshots().len(), 0);
}
