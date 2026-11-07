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

/// A document with two paragraphs, and the store its box tree lives in.
fn two_paragraphs(first: &'static str) -> (Fixture, LayoutStore) {
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("para").text(first),
            Element::new("para").text("bravo"),
        ]),
        "root { display: block; width: 400px }
         para { display: block }",
    );
    let store = fixture.box_tree();
    (fixture, store)
}

/// A label that is cleared keeps its box, and so does everything else.
///
/// A build gives the empty string a box as well, so the patch and the build agree.
#[test]
fn text_that_empties_out_keeps_every_box() {
    let (mut fixture, mut store) = two_paragraphs("alpha");
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 400.0);
    let before = names(&store);

    let text = first_text(&fixture.document, fixture.root);
    fixture.edit_and_restyle(|edit| edit.set_text(text, ""));
    let root = fixture.document.root_index().expect("a root element");

    assert_eq!(
        retext(&mut store, &fixture.document, root),
        Retext::Patched(1)
    );
    assert_eq!(names(&store), before);
}

/// A label that is filled again keeps its box, and so does everything else.
#[test]
fn text_that_fills_an_emptied_node_keeps_every_box() {
    let (mut fixture, mut store) = two_paragraphs("");
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 400.0);
    let before = names(&store);

    let text = first_text(&fixture.document, fixture.root);
    fixture.edit_and_restyle(|edit| edit.set_text(text, "alpha"));
    let root = fixture.document.root_index().expect("a root element");

    assert_eq!(
        retext(&mut store, &fixture.document, root),
        Retext::Patched(1)
    );
    assert_eq!(names(&store), before);
}

/// Text written inside a subtree that generates nothing owes no box and no rebuild.
#[test]
fn text_under_a_hidden_element_keeps_every_box() {
    let mut fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("para").classes(&["gone"]).text("alpha"),
            Element::new("para").text("bravo"),
        ]),
        "root { display: block; width: 400px }
         para { display: block }
         .gone { display: none }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 400.0);
    let before = names(&store);

    let text = first_text(&fixture.document, fixture.root);
    fixture.edit_and_restyle(|edit| edit.set_text(text, "charlie"));
    let root = fixture.document.root_index().expect("a root element");

    assert_eq!(
        retext(&mut store, &fixture.document, root),
        Retext::Patched(0)
    );
    assert_eq!(names(&store), before);
}

/// A font that moved keeps every box: what changes is what the text is shaped into, and that
/// is thrown away and shaped again where the boxes stand.
///
/// The names are half the assertion, as above. The other half is the cold twin: a tree built
/// from scratch at the new size lays its lines out exactly where the patched tree does.
#[test]
fn changing_the_font_size_keeps_every_box_and_shapes_the_new_size() {
    let mut fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("para").text("alpha bravo delta gamma kappa sigma omega alpha bravo"),
        ]),
        "root { display: block; width: 200px; font-size: 16px }
         para { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 400.0);

    let before = names(&store);
    let first_line = lines(&store)[0].clone();
    let shapes_before = content.shaper().shapes;

    let root = fixture.root;
    fixture.edit_restyle_and_patch(&mut store, |edit| {
        edit.set_style_property(root, "font-size", Some("24px"));
    });
    let root_index = fixture.document.root_index().expect("a root element");
    assert_eq!(
        retext(&mut store, &fixture.document, root_index),
        Retext::Patched(0),
        "a font that moved is not a box that has to be built again"
    );
    assert_eq!(
        names(&store),
        before,
        "the patch replaced boxes instead of keeping them, so every downstream name is new"
    );

    lay_out(&mut store, &mut content, 400.0, 400.0);
    assert!(
        content.shaper().shapes > shapes_before,
        "the text was never shaped at the new size: the context is still holding the form it \
         was flattened into at the old one"
    );
    let after = lines(&store)[0].clone();
    assert!(
        after.text.end < first_line.text.end,
        "the first line still holds {:?} of the string, which is the old size's break",
        after.text
    );

    let mut twin = fixture.box_tree();
    let mut twin_content = measurer();
    lay_out(&mut twin, &mut twin_content, 400.0, 400.0);
    let placed = |store: &LayoutStore| {
        lines(store)
            .iter()
            .map(|line| (line.text.clone(), line.top, line.width, line.offset))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        placed(&store),
        placed(&twin),
        "the patched tree lays its lines out somewhere a tree built at the new size does not"
    );
}

/// Nine hundred rows mounted at once under a list are one container's splice, not a rebuild.
///
/// The rows that were there keep their boxes and everything named by them; what is built is the
/// rows that arrived.
#[test]
fn mounting_many_rows_under_one_container_splices_it_and_keeps_the_rest() {
    const KEPT: usize = 100;
    const BORN: usize = 900;
    let rows: Vec<Element> = (0..KEPT)
        .map(|_| Element::new("row").children(vec![Element::new("cell").text("alpha")]))
        .collect();
    let mut fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("column").children(rows)]),
        "root { display: block; width: 300px }
         column { display: flex; flex-direction: column }
         row { display: flex; height: 20px; flex-shrink: 0 }
         cell { display: block; width: 100px; overflow: hidden }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 300.0, 2_000.0);
    // The first cascade's own obligations are what a frame retires when it builds the tree.
    let root = fixture.document.root_index().expect("a root element");
    let _ = zgui_layout::boxtree::retire(&mut fixture.document, root);
    let before = names(&store);

    let column = {
        let store = fixture.document.store();
        store.core(fixture.root).first_child().expect("the column")
    };
    fixture.edit_and_restyle(|edit| {
        for _ in 0..BORN {
            let row = edit.create_element(zgui_interned::ElementName::new("row"));
            let cell = edit.create_element(zgui_interned::ElementName::new("cell"));
            let text = edit.create_text("bravo");
            edit.insert_before(cell, text, None);
            edit.insert_before(row, cell, None);
            edit.insert_before(column, row, None);
        }
    });
    let root = fixture.document.root_index().expect("a root element");
    let owed = zgui_layout::boxtree::retire(&mut fixture.document, root);
    let spliced = zgui_layout::boxtree::patch::rebuild(&mut store, &fixture.document, &owed);
    assert!(
        spliced.is_some(),
        "{} marks under one container fell back to building the whole tree",
        owed.len()
    );
    let after = names(&store);
    assert!(
        before.iter().all(|key| after.contains(key)),
        "a row that was there lost its boxes to the rows that arrived"
    );
    // A row, its cell, the cell's inline root and the run of text: four boxes per row.
    assert_eq!(after.len(), before.len() + BORN * 4);
}
