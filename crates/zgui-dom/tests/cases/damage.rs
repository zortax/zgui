//! Does a freshly inserted subtree get any layout obligation at all?
//!
//! The style engine accumulates no damage for a first-time cascade: it returns before accumulating
//! anything when there is no old style to compare against, and its per-element damage starts empty.
//! So an element being styled for the first time comes out of a real restyle with an empty damage
//! word — measured below rather than assumed, because everything downstream rests on it.
//!
//! Nothing else supplies the missing obligation either. The child-list protocol marks the *parent*,
//! and the rule that every layout and paint obligation is derived from the style engine forbids the
//! insertion from marking the new content itself. The result, without the first-time branch, is a
//! mounted subtree that is never laid out and never appears — no error, no log, nothing on screen.
//!
//! The case that a fix at the point of mutation would miss is here too: a subtree coming back out of
//! `display: none` has had its style data thrown away, so every element in it is styled for the
//! first time again, with no mutation anywhere in it.

use zgui_bits::Dirty;
use zgui_dom::{EverythingMatters, NodeIndex};
use zgui_interned::{ClassName, ElementName};

use crate::support::damage::{translate, translate_without_initial_branch};
use crate::support::engine::Pass;
use crate::support::rows::Rows;
use crate::support::{edit, read};

/// The record a pass made for the element at `index`.
fn record_for(pass: &Pass, index: NodeIndex) -> crate::support::traversal::Restyled {
    *pass
        .records
        .iter()
        .find(|record| record.node == index.get())
        .unwrap_or_else(|| panic!("the traversal never visited slot {}", index.get()))
}

#[test]
fn an_inserted_subtree_gets_its_layout_obligation_only_from_the_first_time_branch() {
    const SHEET: &str = ".row { border-top-left-radius: 4px }";
    let mut table = Rows::new(4);
    let mut engine = table.styled(SHEET);
    edit::retire(&mut table.document);

    let (host, inner) = table
        .document
        .edit(&EverythingMatters, |batch| {
            let host = batch.create_element(ElementName::new("li"));
            batch.set_classes(host, &[ClassName::new("row")]);
            let inner = batch.create_element(ElementName::new("span"));
            batch.insert_before(host, inner, None);
            batch.insert_before(table.container, host, None);
            (host, inner)
        })
        .expect("the document is not poisoned");

    let pass = engine.restyle(&mut table.document, None);
    let host_record = record_for(&pass, host);
    let inner_record = record_for(&pass, inner);

    assert!(host_record.initial && inner_record.initial);
    assert!(
        host_record.damage.is_empty() && inner_record.damage.is_empty(),
        "the engine accumulates nothing for a first-time cascade, which is why the branch exists"
    );
    assert!(
        translate_without_initial_branch(&host_record).is_clean(),
        "without the branch there is no other source, and the new content is never laid out"
    );
    assert!(translate(&host_record).contains(Dirty::RELAYOUT | Dirty::REBUILD_BOX));
    assert!(translate(&inner_record).contains(Dirty::RELAYOUT | Dirty::REBUILD_BOX));

    // The insertion itself marked the parent and the new content, and neither mark is a layout one:
    // that is the rule the branch exists to satisfy rather than to work around.
    let owed = table.document.store().core(host).dirty().own();
    assert!(owed.contains(Dirty::RESTYLE));
    assert!(!owed.intersects(Dirty::RELAYOUT | Dirty::REBUILD_BOX));

    // And the content really is styled, so the missing obligation is the only thing that was wrong.
    assert_eq!(read::radius(&table.document, host), 4.0);
}

/// The case a fix at the point of mutation cannot see: nothing was inserted and nothing was
/// removed, and yet every element of the subtree is styled for the first time again.
#[test]
fn a_subtree_leaving_display_none_is_styled_from_scratch_with_no_mutation_in_it() {
    const SHEET: &str = ".panel { display: none } .panel.shown { display: block }";
    let mut table = Rows::new(1);
    let panel = table.rows[0];
    let inner = table
        .document
        .edit(&EverythingMatters, |batch| {
            let inner = batch.create_element(ElementName::new("span"));
            batch.insert_before(panel, inner, None);
            inner
        })
        .expect("the document is not poisoned");
    table
        .document
        .edit(&EverythingMatters, |batch| {
            batch.set_classes(panel, &[ClassName::new("panel")]);
        })
        .expect("the document is not poisoned");

    let mut engine = table.styled(SHEET);
    assert!(table.document.node(inner).primary_style().is_none());

    table
        .document
        .edit(&EverythingMatters, |batch| {
            batch.set_classes(panel, &[ClassName::new("panel"), ClassName::new("shown")]);
        })
        .expect("the document is not poisoned");
    let pass = engine.restyle(&mut table.document, None);

    let inner_record = record_for(&pass, inner);
    assert!(
        inner_record.initial,
        "the engine threw the subtree's style data away on the way into `display: none`"
    );
    assert!(inner_record.damage.is_empty());
    assert!(translate_without_initial_branch(&inner_record).is_clean());
    assert!(translate(&inner_record).contains(Dirty::RELAYOUT | Dirty::REBUILD_BOX));
}

/// The counterpart, so the branch is not simply "everything always relayouts": an element that
/// already had a style and whose change is paint-shaped gets no layout obligation from here.
#[test]
fn a_second_pass_over_the_same_element_is_not_an_initial_cascade() {
    const SHEET: &str = ".row { color: rgb(1, 1, 1) } .row.hot { color: rgb(9, 0, 0) }";
    let mut table = Rows::new(3);
    let mut engine = table.styled(SHEET);
    edit::retire(&mut table.document);

    let row = table.rows[1];
    table
        .document
        .edit(&EverythingMatters, |batch| {
            batch.add_class(row, ClassName::new("hot"));
        })
        .expect("the document is not poisoned");
    let pass = engine.restyle(&mut table.document, None);

    let record = record_for(&pass, row);
    assert!(!record.initial);
    assert!(
        translate(&record).is_clean(),
        "a colour change is not layout-shaped, and the engine's own bits do not claim it is"
    );
    assert_eq!(read::color(&table.document, row), (9, 0, 0));
}
