//! What the record has to get right, asserted against a real document.

use zgui_interned::ElementName;

use super::DirtyChildren;
use crate::arena::document::Document;
use crate::id::node_key::NodeIndex;
use crate::node::kind::NodeKind;

#[test]
fn a_text_child_survives_the_promotion_to_a_span() {
    // The record has to name every kind of child, not only the ones selectors match: editing
    // the text of a node is an obligation, and a span that could not reach a text node would
    // drop it the moment a fifth mark arrived on the same child list.
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    let mut children = Vec::new();
    for index in 0..12 {
        let kind = if index % 2 == 0 {
            NodeKind::Element
        } else {
            NodeKind::Text
        };
        children.push(document.append(root, kind, ElementName::new("child")));
    }

    let record = DirtyChildren::empty();
    for index in [0, 2, 4, 6, 7] {
        record.widen(root, children[index], document.store());
    }
    assert!(record.is_span());

    let visited: Vec<_> = record.iter(document.store(), root).collect();
    assert!(
        visited.contains(&children[7]),
        "the text node the fifth mark landed on is still reached"
    );
    assert_eq!(visited.first(), Some(&children[0]));
    assert_eq!(visited.last(), Some(&children[7]));
}

/// A document whose root has `width` element children, with the children's indices.
fn row(width: usize) -> (Document, NodeIndex, Vec<NodeIndex>) {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    let children = (0..width)
        .map(|_| document.append(root, NodeKind::Element, ElementName::new("row")))
        .collect();
    (document, root, children)
}

#[test]
fn four_scattered_children_are_named_exactly() {
    let (document, root, children) = row(64);
    let record = DirtyChildren::empty();
    for index in [3, 17, 40, 63] {
        record.widen(root, children[index], document.store());
    }
    assert_eq!(record.exact_len(), Some(4));
    assert!(!record.is_span());

    let mut visited: Vec<_> = record.iter(document.store(), root).collect();
    visited.sort();
    let mut expected: Vec<_> = [3, 17, 40, 63].iter().map(|i| children[*i]).collect();
    expected.sort();
    assert_eq!(visited, expected);
}

#[test]
fn the_fifth_child_promotes_to_the_span_that_covers_them_all() {
    let (document, root, children) = row(32);
    let record = DirtyChildren::empty();
    for index in [5, 9, 2, 20, 11] {
        record.widen(root, children[index], document.store());
    }
    assert!(record.is_span());
    assert_eq!(record.exact_len(), None);

    let visited: Vec<_> = record.iter(document.store(), root).collect();
    assert_eq!(visited.first(), Some(&children[2]));
    assert_eq!(visited.last(), Some(&children[20]));
    assert_eq!(visited.len(), 19, "the span covers everything between");
}

#[test]
fn a_run_growing_along_a_wide_row_costs_one_link_comparison_per_child() {
    // The shape a list being marked in document order takes, and the one an implementation that
    // searches the child list for each new entry turns from milliseconds into minutes: twenty
    // thousand children, each one extending the run by a single link.
    let (document, root, children) = row(20_000);
    let record = DirtyChildren::empty();
    for child in &children {
        record.widen(root, *child, document.store());
    }
    assert!(record.is_span());
    let covered: Vec<_> = record.iter(document.store(), root).collect();
    assert_eq!(covered.len(), children.len());
    assert_eq!(covered.first(), children.first());
    assert_eq!(covered.last(), children.last());
}

#[test]
fn a_child_already_inside_a_run_leaves_it_alone() {
    let (document, root, children) = row(64);
    let record = DirtyChildren::empty();
    for index in [0, 10, 20, 30, 40] {
        record.widen(root, children[index], document.store());
    }
    assert!(record.is_span());
    record.widen(root, children[25], document.store());
    record.widen(root, children[0], document.store());
    record.widen(root, children[40], document.store());

    let repr = record.0.get();
    assert_eq!(repr.slots[0].get(), Some(children[0]));
    assert_eq!(repr.slots[1].get(), Some(children[40]));
}

#[test]
fn marking_the_same_child_repeatedly_does_not_promote() {
    let (document, root, children) = row(8);
    let record = DirtyChildren::empty();
    for _ in 0..16 {
        record.widen(root, children[3], document.store());
    }
    assert_eq!(record.exact_len(), Some(1));
}

#[test]
fn a_child_that_left_the_parent_is_not_yielded() {
    let (document, root, children) = row(4);
    let other = document.store().core(root).parent().expect("has a parent");
    let record = DirtyChildren::empty();
    record.widen(root, children[1], document.store());
    assert_eq!(record.iter(document.store(), root).count(), 1);
    assert_eq!(
        record.iter(document.store(), other).count(),
        0,
        "asking on behalf of a different parent yields nothing"
    );
}

#[test]
fn an_empty_record_yields_nothing_and_clearing_returns_to_empty() {
    let (document, root, children) = row(4);
    let record = DirtyChildren::empty();
    assert!(record.is_empty());
    assert_eq!(record.iter(document.store(), root).count(), 0);
    record.widen(root, children[0], document.store());
    assert!(!record.is_empty());
    record.clear();
    assert!(record.is_empty());
}

#[test]
fn a_run_whose_first_entry_is_removed_still_reaches_the_rest_of_it() {
    // Unlinking clears the node's own sibling links, so a run that started there would walk out
    // of the child list on its first step. Every child it covered would be unreachable, and the
    // obligations on them would be serviced by nothing at all — no panic, no counter, no log.
    let (document, root, children) = row(16);
    let record = document.store().core(root).dirty_children();
    for index in [2, 4, 6, 8, 10] {
        record.widen(root, children[index], document.store());
    }
    assert!(record.is_span());

    crate::node::links::unlink(document.store(), children[2]);
    let visited: Vec<_> = record.iter(document.store(), root).collect();
    assert!(visited.contains(&children[10]));
    assert!(!visited.contains(&children[2]));
    assert_eq!(visited.first(), Some(&children[3]));
}

#[test]
fn a_run_whose_last_entry_is_removed_re_anchors_onto_the_child_before_it() {
    // Not for the sake of this walk — one that never meets its end simply runs to the end of
    // the child list — but for the sake of the next widening. A run still naming a child that
    // has left claims that child is inside it, so putting the child back anywhere earlier in
    // the list leaves it named, unreachable, and owing work nothing leads to.
    let (document, root, children) = row(16);
    let record = document.store().core(root).dirty_children();
    for index in [2, 4, 6, 8, 10] {
        record.widen(root, children[index], document.store());
    }
    assert!(record.is_span());

    crate::node::links::unlink(document.store(), children[10]);
    crate::node::links::link_before(document.store(), root, children[10], Some(children[0]));
    record.widen(root, children[10], document.store());

    let visited: Vec<_> = record.iter(document.store(), root).collect();
    assert!(visited.contains(&children[10]));
    assert!(visited.contains(&children[2]));
    assert!(visited.contains(&children[9]));
}

#[test]
fn a_run_reduced_to_its_last_child_names_nothing_once_that_child_goes() {
    let (document, root, children) = row(8);
    let record = document.store().core(root).dirty_children();
    for index in [3, 4, 5, 6, 7] {
        record.widen(root, children[index], document.store());
    }
    for index in [3, 4, 5, 6] {
        crate::node::links::unlink(document.store(), children[index]);
    }
    assert_eq!(
        record.iter(document.store(), root).collect::<Vec<_>>(),
        vec![children[7]]
    );

    crate::node::links::unlink(document.store(), children[7]);
    assert_eq!(record.iter(document.store(), root).count(), 0);
}

#[test]
fn a_mark_for_a_child_the_owner_does_not_parent_is_ignored() {
    // A run is described by two ends of one child list. Widening for a child of a *different*
    // list would re-anchor it onto that list, and every child this record was recording would
    // become unreachable at once — the run leads somewhere else and the parent test at the end
    // of `iter` drops everything it yields.
    let (document, root, children) = row(16);
    let record = DirtyChildren::empty();
    for index in [1, 3, 5, 7, 9] {
        record.widen(root, children[index], document.store());
    }
    assert!(record.is_span());

    let elsewhere = document.store().core(root).parent().expect("has a parent");
    record.widen(root, elsewhere, document.store());

    let visited: Vec<_> = record.iter(document.store(), root).collect();
    assert_eq!(visited.first(), Some(&children[1]));
    assert_eq!(visited.last(), Some(&children[9]));
}

#[test]
fn marks_clustered_inside_a_run_keep_it_as_narrow_as_it_was() {
    // The shape a virtualised list takes: a window of rows changes inside a list far wider than
    // the window. Every one of these sits within reach of an end of the run, so the run stays
    // the window rather than growing to the list.
    let (document, root, children) = row(4_000);
    let record = DirtyChildren::empty();
    for index in [2_000, 2_001, 2_002, 2_003, 2_040] {
        record.widen(root, children[index], document.store());
    }
    assert!(record.is_span());
    for index in [2_010, 2_020, 2_035, 2_004] {
        record.widen(root, children[index], document.store());
    }

    let visited: Vec<_> = record.iter(document.store(), root).collect();
    assert_eq!(visited.first(), Some(&children[2_000]));
    assert_eq!(visited.last(), Some(&children[2_040]));
    assert_eq!(visited.len(), 41);
}

#[test]
fn a_mark_too_far_from_the_run_to_place_widens_to_every_child() {
    // The bound, stated as behaviour. Nothing marked is lost — the record still names every
    // child that owes work — but the description stops being the run between them, because
    // finding out where the child sits would cost a walk of the child list on every mark.
    let (document, root, children) = row(4_000);
    let record = DirtyChildren::empty();
    for index in [2_000, 2_001, 2_002, 2_003, 2_004] {
        record.widen(root, children[index], document.store());
    }
    assert_eq!(record.iter(document.store(), root).count(), 5);

    record.widen(root, children[3_500], document.store());
    let visited: Vec<_> = record.iter(document.store(), root).collect();
    assert_eq!(visited.len(), children.len(), "every child, and no more");
    assert_eq!(visited.first(), children.first());
    assert_eq!(visited.last(), children.last());
    assert!(visited.contains(&children[3_500]));
}

#[test]
fn replacing_rebuilds_the_record_from_scratch() {
    let (document, root, children) = row(16);
    let record = DirtyChildren::empty();
    record.widen(root, children[0], document.store());
    record.replace(root, [children[7], children[9]], document.store());
    let mut visited: Vec<_> = record.iter(document.store(), root).collect();
    visited.sort();
    assert_eq!(visited, vec![children[7], children[9]]);
}
