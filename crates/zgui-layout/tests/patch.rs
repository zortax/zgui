//! What changing a document costs the box tree that was built from it.
//!
//! A box's name is what fragment reuse, geometry diffing and damage scissoring are keyed on, so
//! "the tree was rebuilt" and "the tree was patched" are not two ways of getting the same answer:
//! the first makes every downstream cache miss. Every assertion here is therefore about the names
//! as much as about the content — that the boxes are the ones that were already there, *and* that
//! what they now hold is what the document says.

mod support;

use support::text::{first_inline_root, lines};
use support::{Element, Fixture, lay_out, measurer};
use zgui_dom::{NodeIndex, NodeKind};
use zgui_layout::BoxKey;
use zgui_layout::boxtree::patch::{Retext, retext};
use zgui_layout::tree::store::LayoutStore;

/// The first text node under `index`, in document order.
fn first_text(document: &zgui_dom::Document, index: NodeIndex) -> NodeIndex {
    search(document, index).expect("the fixture has no text node")
}

/// The same, answering nothing for a subtree with no text in it.
fn search(document: &zgui_dom::Document, index: NodeIndex) -> Option<NodeIndex> {
    if document.store().core(index).kind() == NodeKind::Text {
        return Some(index);
    }
    let mut next = document.store().core(index).first_child();
    while let Some(child) = next {
        if let Some(found) = search(document, child) {
            return Some(found);
        }
        next = document.store().core(child).next_sibling();
    }
    None
}

/// Every box in `store`, so that two sets of names can be compared.
fn names(store: &LayoutStore) -> Vec<BoxKey> {
    let mut keys = store.keys();
    keys.sort_by_key(|key| (key.index(), key.generation()));
    keys
}

/// A document with one paragraph, and the store its box tree lives in.
fn fixture(text: &'static str) -> (Fixture, LayoutStore) {
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("para").text(text)]),
        "root { display: block; width: 400px }
         para { display: block }",
    );
    let store = fixture.box_tree();
    (fixture, store)
}

/// A change of characters is a change to one box, and the tree above it is the tree that was there.
///
/// The names are half the assertion and the content is the other half. Keeping the names while
/// laying out the old string is precisely the failure the flattened form of an inline formatting
/// context produces if it is not dropped — it is checked against the *sequence of boxes* it was
/// built from, and a box rewritten where it stands is the same box in the same position.
#[test]
fn rewriting_a_text_node_keeps_every_box_and_lays_out_the_new_characters() {
    let (mut fixture, mut store) = fixture("alpha");
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 400.0);

    let before = names(&store);
    let first_line = lines(&store)[0].clone();
    let shapes_before = content.shaper().shapes;

    let text = first_text(&fixture.document, fixture.root);
    fixture.edit_and_restyle(|edit| edit.set_text(text, "alpha bravo delta gamma"));

    let root = fixture.document.root_index().expect("a root element");
    assert_eq!(
        retext(&mut store, &fixture.document, root),
        Retext::Patched(1),
        "one text node changed, so exactly one box should have been rewritten"
    );
    assert_eq!(
        names(&store),
        before,
        "the patch replaced boxes instead of rewriting one, so every downstream name is new"
    );

    lay_out(&mut store, &mut content, 400.0, 400.0);
    assert!(
        content.shaper().shapes > shapes_before,
        "the new characters were never shaped: the context is still holding the form it was \
         flattened into before the rewrite"
    );
    let after = lines(&store)[0].clone();
    assert!(
        after.width > first_line.width && after.text.end > first_line.text.end,
        "the paragraph laid out as {} wide over {:?} bytes, which is the string it held before the \
         rewrite",
        after.width,
        after.text
    );
}

/// Writing the same characters again rewrites nothing, so nothing downstream is invalidated.
#[test]
fn writing_the_characters_a_box_already_holds_is_not_a_rewrite() {
    let (mut fixture, mut store) = fixture("alpha");
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 400.0);
    let context = first_inline_root(&store);

    // Marked by hand, because the document's own edit path returns early for an unchanged string
    // and this is about the *patch* refusing to do work rather than about the edit refusing to.
    let text = first_text(&fixture.document, fixture.root);
    zgui_dom::dirty::propagate::mark(
        fixture.document.store_mut(),
        text,
        zgui_bits::Dirty::RESHAPE,
    );

    let root = fixture.document.root_index().expect("a root element");
    assert_eq!(
        retext(&mut store, &fixture.document, root),
        Retext::Patched(0)
    );
    assert!(
        store.inline_resolution(context).is_some(),
        "a rewrite that changed nothing threw the context's lines away anyway"
    );
}

/// Text that disappears is a box that disappears, and the patch says so instead of guessing.
///
/// An empty text node generates no box at all, so servicing this in place would mean creating and
/// destroying boxes — and with them anonymous wrapping, inline splitting and paint order. The
/// answer that keeps the tree honest is to refuse, and the caller rebuilds.
#[test]
fn text_that_empties_out_is_refused_rather_than_approximated() {
    let (mut fixture, mut store) = fixture("alpha");
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 400.0);

    let text = first_text(&fixture.document, fixture.root);
    fixture.edit_and_restyle(|edit| edit.set_text(text, ""));

    let root = fixture.document.root_index().expect("a root element");
    assert_eq!(retext(&mut store, &fixture.document, root), Retext::Rebuild);
}
