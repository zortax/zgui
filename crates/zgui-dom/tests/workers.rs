//! The cell discipline under the traffic it exists for.
//!
//! A style traversal hands one element to each of several workers and each worker walks *outwards*
//! from its own: to the parent for a descendant combinator, back along the element chain for a
//! sibling one, down for a structural pseudo-class. Every worker is therefore reading records that
//! other workers are standing on, and two of them write — selector flags land on the element being
//! matched *and on its parent*, from whichever thread happens to hold the child.
//!
//! An interpreter cannot reach that: it runs one thread. So this file runs the reads and the writes
//! for real, over a shared document, from many threads at once, with an oracle computed
//! single-threaded beforehand. What it would catch is a field that answers differently under
//! contention — which is what a borrow counter behind a shared reference does, silently, because
//! the counter is a read-modify-write and both racing accesses are logically reads.

use std::sync::atomic::{AtomicUsize, Ordering};

use selectors::matching::ElementSelectorFlags;
use zgui_dom::{Document, NodeIndex, NodeKind};
use zgui_interned::{ClassName, ElementName, Ident};
use zgui_vocab::UiState;

/// How many threads walk the document at once.
const WORKERS: usize = 8;

/// How many times each worker walks it.
const ROUNDS: usize = 64;

/// A document of `rows` rows, each holding a label, a text node and a value.
fn grid(rows: usize) -> (Document, Vec<NodeIndex>) {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    document.set_classes(root, &[ClassName::new("grid")]);

    let mut leaves = Vec::new();
    for index in 0..rows {
        let row = document.append(root, NodeKind::Element, ElementName::new("row"));
        document.set_classes(row, &[ClassName::new("row"), ClassName::new("striped")]);
        document.set_id(row, Some(Ident::new("row")));
        if index % 3 == 0 {
            document.set_state(row, UiState::HOVER | UiState::ENABLED);
        }

        let label = document.append(row, NodeKind::Element, ElementName::new("label"));
        document.set_classes(label, &[ClassName::new("label")]);
        document.append(row, NodeKind::Text, ElementName::new("#text"));
        let value = document.append(row, NodeKind::Element, ElementName::new("value"));
        document.set_classes(value, &[ClassName::new("value")]);
        leaves.push(label);
        leaves.push(value);
    }
    (document, leaves)
}

/// Everything selector matching reads about one node, folded into a number.
///
/// The fold is deliberately over *all* of it — the walk outwards, the names, the classes, the
/// identifier, the state — so that a field answering differently under contention changes the
/// result rather than being averaged away.
fn probe(document: &Document, node: NodeIndex) -> u64 {
    let store = document.store();
    let record = store.core(node);
    let mut digest = 0u64;

    digest = digest.wrapping_mul(31).wrapping_add(record.kind() as u64);
    digest = digest
        .wrapping_mul(31)
        .wrapping_add(record.local_name().as_str().len() as u64);
    digest = digest.wrapping_mul(31).wrapping_add(record.state().bits());
    digest = digest
        .wrapping_mul(31)
        .wrapping_add(store.classes_of(node).len() as u64);
    for class in store.classes_of(node) {
        digest = digest
            .wrapping_mul(31)
            .wrapping_add(class.to_string().len() as u64);
    }
    digest = digest
        .wrapping_mul(31)
        .wrapping_add(record.id_attr().map_or(0, |id| id.as_str().len() as u64));
    digest = digest
        .wrapping_mul(31)
        .wrapping_add(record.child_count() as u64);

    // Outwards: parent, both element siblings, the first element child. These are the reads that
    // touch a record another worker is standing on.
    for neighbour in [
        record.parent(),
        record.prev_element(),
        record.next_element(),
        record.first_element_child(),
    ] {
        digest = digest.wrapping_mul(31).wrapping_add(match neighbour {
            Some(index) => {
                let other = store.core(index);
                (other.local_name().as_str().len() as u64)
                    .wrapping_mul(7)
                    .wrapping_add(store.classes_of(index).len() as u64)
                    .wrapping_add(other.state().bits())
                    .wrapping_add(1)
            }
            None => 0,
        });
    }

    // Upwards to the root, the way a descendant combinator walks.
    let mut current = record.parent();
    while let Some(index) = current {
        let ancestor = store.core(index);
        digest = digest
            .wrapping_mul(31)
            .wrapping_add(store.classes_of(index).len() as u64);
        current = ancestor.parent();
    }

    digest
}

#[test]
fn many_workers_reading_one_document_all_see_the_same_thing() {
    let (document, leaves) = grid(128);
    let oracle: Vec<u64> = leaves.iter().map(|node| probe(&document, *node)).collect();

    let document = &document;
    let leaves = &leaves;
    let oracle = &oracle;
    let mismatches = AtomicUsize::new(0);
    let mismatches = &mismatches;

    std::thread::scope(|scope| {
        for worker in 0..WORKERS {
            scope.spawn(move || {
                for round in 0..ROUNDS {
                    // Each worker starts at a different offset, so the workers are on different
                    // records at any instant and their walks overlap on shared ancestors.
                    let offset = (worker * 17 + round) % leaves.len();
                    for step in 0..leaves.len() {
                        let position = (offset + step) % leaves.len();
                        if probe(document, leaves[position]) != oracle[position] {
                            mismatches.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            });
        }
    });

    assert_eq!(
        mismatches.load(Ordering::Relaxed),
        0,
        "a read of a shared record answered differently under contention"
    );
}

#[test]
fn selector_flags_written_on_self_and_on_the_parent_from_every_worker_all_survive() {
    let (document, leaves) = grid(64);
    let document = &document;
    let leaves = &leaves;

    // One flag per worker, so the union at the end says exactly which writes were kept.
    let flags = [
        ElementSelectorFlags::HAS_SLOW_SELECTOR,
        ElementSelectorFlags::HAS_SLOW_SELECTOR_LATER_SIBLINGS,
        ElementSelectorFlags::HAS_EDGE_CHILD_SELECTOR,
        ElementSelectorFlags::HAS_EMPTY_SELECTOR,
        ElementSelectorFlags::ANCHORS_RELATIVE_SELECTOR,
        ElementSelectorFlags::ANCHORS_RELATIVE_SELECTOR_NON_SUBJECT,
        ElementSelectorFlags::RELATIVE_SELECTOR_SEARCH_DIRECTION_ANCESTOR,
        ElementSelectorFlags::RELATIVE_SELECTOR_SEARCH_DIRECTION_SIBLING,
    ];
    assert_eq!(flags.len(), WORKERS, "one distinct flag per worker");

    std::thread::scope(|scope| {
        for flag in flags {
            scope.spawn(move || {
                let store = document.store();
                for _ in 0..ROUNDS {
                    for leaf in leaves {
                        let record = store.core(*leaf);
                        record.insert_selector_flags(flag);
                        if let Some(parent) = record.parent() {
                            // The write that forces the field to be an atomic: a worker holding a
                            // child reaches up and writes on a record every other worker is also
                            // reading and writing.
                            store.core(parent).insert_selector_flags(flag);
                        }
                    }
                }
            });
        }
    });

    let union = flags
        .iter()
        .copied()
        .fold(ElementSelectorFlags::empty(), |all, flag| all | flag);
    let store = document.store();
    for leaf in leaves {
        assert_eq!(
            store.core(*leaf).selector_flags().bits(),
            union.bits(),
            "every worker's write survived on the element it matched"
        );
        let parent = store.core(*leaf).parent().expect("a leaf has a parent");
        assert_eq!(
            store.core(parent).selector_flags().bits(),
            union.bits(),
            "and on the parent, which several workers wrote at once"
        );
    }
}

#[test]
fn a_worker_can_hold_a_record_while_the_document_is_read_from_every_other_one() {
    // The property the arena exists for, stated where it matters: a reference handed to a worker
    // stays pointing at the same record for as long as the worker holds it.
    let (document, leaves) = grid(64);
    let document = &document;
    let leaves = &leaves;

    std::thread::scope(|scope| {
        for worker in 0..WORKERS {
            scope.spawn(move || {
                let held = document.node(leaves[worker]);
                let address = core::ptr::from_ref(held.record()) as usize;
                for _ in 0..ROUNDS {
                    for leaf in leaves {
                        let _ = probe(document, *leaf);
                    }
                    assert_eq!(
                        core::ptr::from_ref(held.record()) as usize,
                        address,
                        "the record a worker is holding did not move"
                    );
                }
            });
        }
    });
}

/// The compile-time half, which is the half that catches the mistake before it can race.
///
/// Nothing here runs; the assertions are discharged while the file is compiled. What they say is
/// that the whole store is safe to share, so every column is, and that the record's own field
/// shapes are the three permitted ones.
mod the_discipline_is_checked_by_the_compiler {
    use core::cell::Cell;
    use core::sync::atomic::{AtomicI32, AtomicU32, AtomicU64};

    use zgui_dom::{CellDisciplined, DocumentStore};

    const fn disciplined<T: CellDisciplined>() {}

    const _: () = zgui_dom::assert_sync::<DocumentStore>();
    const _: () = disciplined::<Cell<u32>>();
    const _: () = disciplined::<AtomicU32>();
    const _: () = disciplined::<AtomicU64>();
    const _: () = disciplined::<AtomicI32>();

    #[test]
    fn the_document_and_its_store_are_shareable() {
        fn assert_shareable<T: Send + Sync>() {}
        assert_shareable::<DocumentStore>();
        assert_shareable::<zgui_dom::Document>();
    }
}
