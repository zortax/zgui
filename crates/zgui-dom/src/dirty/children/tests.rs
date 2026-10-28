//! What the record has to get right, asserted against a real document.

use zgui_interned::ElementName;

use super::{DirtyChildren, EXACT, OVERFLOW_CAP};
use crate::arena::document::Document;
use crate::id::node_key::NodeIndex;
use crate::node::kind::NodeKind;

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

/// The children the record names, sorted.
fn named(record: &DirtyChildren, document: &Document, owner: NodeIndex) -> Vec<NodeIndex> {
    let mut visited: Vec<_> = record.iter(document.store(), owner).collect();
    visited.sort();
    visited
}

#[test]
fn a_text_child_is_named_like_any_other() {
    // The record has to name every kind of child, not only the ones selectors match: editing
    // the text of a node is an obligation like any other.
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
    assert!(record.is_wide());
    let mut expected: Vec<_> = [0, 2, 4, 6, 7].iter().map(|i| children[*i]).collect();
    expected.sort();
    assert_eq!(named(&record, &document, root), expected);
}

#[test]
fn four_scattered_children_are_named_in_the_record_itself() {
    let (document, root, children) = row(64);
    let record = DirtyChildren::empty();
    for index in [3, 17, 40, 63] {
        record.widen(root, children[index], document.store());
    }
    assert_eq!(record.exact_len(), Some(EXACT));
    assert!(!record.is_wide());

    let mut expected: Vec<_> = [3, 17, 40, 63].iter().map(|i| children[*i]).collect();
    expected.sort();
    assert_eq!(named(&record, &document, root), expected);
}

#[test]
fn the_fifth_child_moves_the_record_to_the_overflow_set_and_names_exactly_the_five() {
    let (document, root, children) = row(32);
    let record = DirtyChildren::empty();
    for index in [5, 9, 2, 20, 11] {
        record.widen(root, children[index], document.store());
    }
    assert!(record.is_wide());
    assert!(!record.is_all());
    assert_eq!(record.exact_len(), None);

    let mut expected: Vec<_> = [5, 9, 2, 20, 11].iter().map(|i| children[*i]).collect();
    expected.sort();
    assert_eq!(named(&record, &document, root), expected);
}

#[test]
fn scattered_marks_across_a_wide_row_name_exactly_the_marked_children() {
    // The shape that used to degrade to every child: marks far apart in a list ten thousand
    // wide. Each costs one insertion, and a walk visits the marked children and no others.
    let (document, root, children) = row(10_000);
    let record = DirtyChildren::empty();
    let marked: Vec<usize> = (0..500).map(|step| step * 7_919 % 10_000).collect();
    for index in &marked {
        record.widen(root, children[*index], document.store());
    }
    let mut expected: Vec<_> = marked.iter().map(|i| children[*i]).collect();
    expected.sort();
    expected.dedup();
    assert_eq!(named(&record, &document, root), expected);
}

#[test]
fn past_the_cap_the_record_names_every_child() {
    let (document, root, children) = row(OVERFLOW_CAP + 8);
    let record = DirtyChildren::empty();
    for child in &children[..=OVERFLOW_CAP] {
        record.widen(root, *child, document.store());
    }
    assert!(record.is_all());
    let mut expected = children.clone();
    expected.sort();
    assert_eq!(
        named(&record, &document, root),
        expected,
        "every child, the unmarked ones included"
    );
    assert!(
        document.store().dirty_overflow().get(&root).is_none(),
        "the set is released once every child is named"
    );
}

#[test]
fn marking_the_same_child_repeatedly_does_not_widen() {
    let (document, root, children) = row(8);
    let record = DirtyChildren::empty();
    for _ in 0..16 {
        record.widen(root, children[3], document.store());
    }
    assert_eq!(record.exact_len(), Some(1));

    for index in [0, 1, 2, 4] {
        record.widen(root, children[index], document.store());
    }
    assert!(record.is_wide());
    for _ in 0..16 {
        record.widen(root, children[3], document.store());
    }
    assert_eq!(named(&record, &document, root).len(), 5);
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
fn a_child_unlinked_after_a_wide_mark_is_filtered_out() {
    let (document, root, children) = row(16);
    let record = document.store().core(root).dirty_children();
    for index in [2, 4, 6, 8, 10] {
        record.widen(root, children[index], document.store());
    }
    assert!(record.is_wide());

    crate::node::links::unlink(document.store(), children[2]);
    let visited = named(record, &document, root);
    assert!(!visited.contains(&children[2]));
    assert_eq!(visited.len(), 4);
}

#[test]
fn a_child_moved_to_the_front_of_its_own_list_stays_named() {
    // The reorder every keyed list does: a marked child taken out and put back earlier. It is
    // named by identity, so where it sits changes nothing about whether it is reached.
    let (document, root, children) = row(16);
    let record = document.store().core(root).dirty_children();
    for index in [2, 4, 6, 8, 10] {
        record.widen(root, children[index], document.store());
    }
    crate::node::links::unlink(document.store(), children[10]);
    crate::node::links::link_before(document.store(), root, children[10], Some(children[0]));
    record.widen(root, children[10], document.store());

    let visited = named(record, &document, root);
    assert!(visited.contains(&children[10]));
    assert_eq!(visited.len(), 5);
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
fn a_mark_for_a_child_the_owner_does_not_parent_is_ignored() {
    let (document, root, children) = row(16);
    let record = DirtyChildren::empty();
    for index in [1, 3, 5, 7, 9] {
        record.widen(root, children[index], document.store());
    }
    let elsewhere = document.store().core(root).parent().expect("has a parent");
    record.widen(root, elsewhere, document.store());
    assert_eq!(named(&record, &document, root).len(), 5);
}

#[test]
fn replacing_rebuilds_the_record_from_scratch_and_releases_the_set() {
    let (document, root, children) = row(16);
    let record = DirtyChildren::empty();
    for index in [0, 1, 2, 3, 4] {
        record.widen(root, children[index], document.store());
    }
    assert!(record.is_wide());
    record.replace(root, [children[7], children[9]], document.store());
    assert_eq!(record.exact_len(), Some(2));
    assert!(document.store().dirty_overflow().get(&root).is_none());
    assert_eq!(
        named(&record, &document, root),
        vec![children[7], children[9]]
    );
}

#[test]
fn a_dropped_owner_takes_its_overflow_set_with_it() {
    // Clearing a wide record leaves its set to the next mark, and an owner that leaves the
    // document is never marked again: without this, every removed row that was ever wide keeps
    // its set for the life of the document.
    let (mut document, root, children) = row(EXACT + 2);
    let record = DirtyChildren::empty();
    for child in &children {
        record.widen(root, *child, document.store());
    }
    assert!(record.is_wide());
    record.clear();
    assert!(document.store().dirty_overflow().get(&root).is_some());

    document
        .edit(&crate::mutate::filter::EverythingMatters, |batch| {
            batch.remove(root);
        })
        .expect("the document is not poisoned");
    crate::arena::recycle::end_frame(&mut document);
    assert!(
        document.store().dirty_overflow().get(&root).is_none(),
        "the set went with its owner"
    );
}
