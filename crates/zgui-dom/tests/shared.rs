//! The paths that write through a *shared* borrow, small enough to run under an interpreter.
//!
//! [`tests/workers`](../workers.rs) runs the same shapes at realistic width, which is what a real
//! traversal looks like and what a sanitiser wants; it is far too big for an interpreter to
//! explore. This file is the other half: two threads, a handful of nodes, and every place in the
//! crate where a method taking `&self` stores something. An interpreter checking for data races
//! sees each of those stores against every read that can be concurrent with it, which is the check
//! no amount of running the wide version at speed will reliably produce.
//!
//! Everything here is deliberately tiny. If it grows past a few hundred nodes it stops running in
//! the interpreter, and then it stops being this test.

use std::sync::atomic::{AtomicUsize, Ordering};

use selectors::matching::ElementSelectorFlags;
use zgui_dom::{Dirty, Document, NodeIndex, NodeKind};
use zgui_interned::ElementName;

/// A root with `width` element children, every second one preceded by a text node.
fn row(width: usize) -> (Document, NodeIndex, Vec<NodeIndex>) {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    let mut children = Vec::new();
    for index in 0..width {
        if index % 2 == 1 {
            document.append(root, NodeKind::Text, ElementName::new("#text"));
        }
        children.push(document.append(root, NodeKind::Element, ElementName::new("item")));
    }
    (document, root, children)
}

#[test]
fn two_readers_may_be_inside_the_lazy_renumbering_at_once() {
    // The position of a node among its element siblings is numbered on demand, on a *shared*
    // borrow — so the first two readers after a structural change are both inside the numbering,
    // writing the same numbers over each other. Plain stores there would be a data race, silently,
    // and only under a load nobody reproduces on purpose.
    let (document, _, children) = row(6);
    let store = document.store();
    let children = &children;

    std::thread::scope(|scope| {
        for worker in 0..2 {
            scope.spawn(move || store.ordinal_of(children[worker * 3]));
        }
    });

    for (position, child) in children.iter().enumerate() {
        assert_eq!(
            store.ordinal_of(*child),
            position as u32,
            "the numbering two readers raced through is still the right one"
        );
    }
}

#[test]
fn a_reader_of_a_position_sees_the_numbering_that_published_it() {
    // One thread asks for a position while another asks for a different one, over and over. What
    // must not happen is a reader finding the epoch current and then reading a position from
    // before it — the numbering is published by a single store, and it is stored last.
    let (document, _, children) = row(4);
    let store = document.store();
    let children = &children;
    let wrong = AtomicUsize::new(0);
    let wrong = &wrong;

    std::thread::scope(|scope| {
        for worker in 0..2 {
            scope.spawn(move || {
                for step in 0..4 {
                    let position = (worker * 2 + step) % children.len();
                    if store.ordinal_of(children[position]) != position as u32 {
                        wrong.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
        }
    });

    assert_eq!(wrong.load(Ordering::Relaxed), 0);
}

#[test]
fn the_words_the_engine_writes_from_a_worker_take_every_write() {
    // Selector flags land on the element being matched *and on its parent*, from whichever thread
    // holds the child. Two threads doing that to one parent at once is the write the field is an
    // atomic for.
    let (document, root, children) = row(4);
    let store = document.store();
    let children = &children;

    let flags = [
        ElementSelectorFlags::HAS_SLOW_SELECTOR,
        ElementSelectorFlags::HAS_EDGE_CHILD_SELECTOR,
    ];
    std::thread::scope(|scope| {
        for (worker, flag) in flags.iter().enumerate() {
            let flag = *flag;
            scope.spawn(move || {
                for child in children.iter().skip(worker).step_by(2) {
                    let record = store.core(*child);
                    record.insert_selector_flags(flag);
                    record.set_atomic(zgui_dom::node::atomics::STYLED);
                    if let Some(parent) = record.parent() {
                        store.core(parent).insert_selector_flags(flag);
                    }
                    record.dirty().mark(Dirty::RESTYLE);
                    store.core(root).dirty().mark_subtree(Dirty::RESTYLE);
                }
            });
        }
    });

    let union = flags[0] | flags[1];
    assert_eq!(store.core(root).selector_flags().bits(), union.bits());
    assert!(store.core(root).has_dirty_descendants(Dirty::RESTYLE));
    for child in children {
        assert!(store.core(*child).is_styled());
    }
}
