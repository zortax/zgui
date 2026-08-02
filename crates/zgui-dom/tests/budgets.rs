//! What the invalidation machinery costs, in counters rather than in seconds.
//!
//! These are the assertions that turn "O(dirty path), not O(document)" into a contract. A timing is
//! a property of the machine; "one leaf written at depth twenty in a fifty-thousand-node tree costs
//! twenty-one nodes visited" is a property of the design, and it stays true on a slow machine, a
//! fast one and under a debugger.
//!
//! Three counters, and each exists because the one before it is silent about a real cost.
//! `nodes_visited` counts the nodes a walk entered, and it stays small even for a walk that probed
//! ten thousand clean siblings to find the one that was dirty — the cost it cannot see is exactly
//! the cost the dirty-child record exists to remove. `dirty_walk_steps` counts every child probed,
//! so it is the counter that separates "descended into the marked children" from "tested every
//! child". Both count what a *walk* does, and neither can see what maintaining the record between
//! walks costs; `dirty_child_steps` counts the sibling links a mark follows, which is where an
//! implementation that walks the child list once per mark shows up.
//!
//! # Why this is a target of its own
//!
//! The counters are process-global. A case that reads one has to be the only thing bumping it, so
//! every counter assertion in the crate lives here, behind one lock, and no other test target reads
//! them at all.

// The engine harness declares `unsafe` where it takes up the style engine's own data-ownership
// contract, and it states its reason there.
#![allow(unsafe_code)]

#[path = "support/mod.rs"]
mod support;

use std::sync::{Mutex, MutexGuard};

use zgui_bits::Dirty;
use zgui_dom::dirty::{propagate, walk};
use zgui_dom::{Document, EverythingMatters, NodeIndex, NodeKind};
use zgui_interned::{ClassName, ElementName};
use zgui_profile::{Counter, counter};
use zgui_vocab::UiState;

use crate::support::edit;
use crate::support::engine::Engine;

/// Held for the whole of any test that reads a counter.
static COUNTERS: Mutex<()> = Mutex::new(());

/// Takes the counter lock and zeroes every counter.
fn measuring() -> MutexGuard<'static, ()> {
    let guard = COUNTERS.lock().unwrap_or_else(|held| held.into_inner());
    counter::reset();
    guard
}

/// A tree of `total` nodes with a chain `depth` deep hanging off the document node.
///
/// The rest of the nodes are a wide subtree beside the chain, so that a walk which is O(document)
/// rather than O(dirty path) has somewhere to be seen doing it.
fn deep_tree(total: usize, depth: usize) -> (Document, NodeIndex) {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    let mut leaf = root;
    for _ in 1..depth {
        leaf = document.append(leaf, NodeKind::Element, ElementName::new("box"));
    }
    let filler = document.append(root, NodeKind::Element, ElementName::new("box"));
    while document.len() < total {
        document.append(filler, NodeKind::Element, ElementName::new("box"));
    }
    assert_eq!(document.len(), total);
    (document, leaf)
}

/// Every live node of `document`, by slot number.
fn every_node(document: &Document) -> Vec<NodeIndex> {
    (0..document.store().slot_count() as u32)
        .map(NodeIndex::new)
        .filter(|index| document.store().try_core(*index).is_some())
        .collect()
}

#[test]
fn walk_is_o_depth() {
    let _measuring = measuring();
    // The leaf sits at depth 20 counting the document node as zero: twenty elements on the chain.
    let (mut document, leaf) = deep_tree(50_000, 20);
    let root = document.document_index();

    propagate::mark(document.store_mut(), leaf, Dirty::RESTYLE);
    let mut serviced = 0;
    walk::walk(document.store_mut(), root, Dirty::RESTYLE, &mut |_, _| {
        serviced += 1
    });

    assert_eq!(counter::get(Counter::NodesVisited), 21);
    assert_eq!(counter::get(Counter::DirtyWalkSteps), 20);
    assert_eq!(serviced, 1);

    // The control, without which the numbers above could mean the walk simply does not work.
    counter::reset();
    for node in every_node(&document) {
        propagate::mark(document.store_mut(), node, Dirty::RESTYLE);
    }
    let mut serviced = 0;
    walk::walk(document.store_mut(), root, Dirty::RESTYLE, &mut |_, _| {
        serviced += 1
    });
    assert_eq!(counter::get(Counter::NodesVisited), 50_000);
    assert_eq!(serviced, 50_000);
}

#[test]
fn an_idle_walk_over_a_drained_tree_visits_nothing() {
    let _measuring = measuring();
    let (mut document, leaf) = deep_tree(10_000, 10);
    let root = document.document_index();
    propagate::mark(document.store_mut(), leaf, Dirty::RESTYLE);
    walk::walk(document.store_mut(), root, Dirty::RESTYLE, &mut |_, _| {});

    counter::reset();
    walk::walk(document.store_mut(), root, Dirty::RESTYLE, &mut |_, _| {});
    assert_eq!(counter::get(Counter::NodesVisited), 0);
    assert_eq!(counter::get(Counter::DirtyWalkSteps), 0);
}

/// A container with `width` element children.
fn wide_row(width: usize) -> (Document, Vec<NodeIndex>) {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    let rows = (0..width)
        .map(|_| document.append(root, NodeKind::Element, ElementName::new("li")))
        .collect();
    (document, rows)
}

/// The child probes one walk costs after marking `marked` of a ten-thousand-wide row.
fn probes_for(marked: &[usize]) -> u64 {
    let (mut document, rows) = wide_row(10_000);
    for index in marked {
        propagate::mark(document.store_mut(), rows[*index], Dirty::RESTYLE);
    }
    counter::reset();
    let root = document.document_index();
    walk::walk(document.store_mut(), root, Dirty::RESTYLE, &mut |_, _| {});
    counter::get(Counter::DirtyWalkSteps)
}

#[test]
fn a_scattered_fifth_mark_promotes_to_the_span() {
    let _measuring = measuring();
    // Four exact children, plus the one probe that leads from the document node into the container.
    assert_eq!(probes_for(&[3, 900, 4_000, 9_999]), 5);
    // The fifth turns the record into the inclusive run that covers all five, which is the right
    // description for a reorder and the wrong one for two scattered pointer moves. Every budget in
    // this file is written knowing that, which is why none of them uses five scattered marks.
    assert!(probes_for(&[3, 900, 4_000, 9_000, 9_999]) > 9_000);
}

/// The order marks arrive in, and the sibling steps recording them cost.
///
/// A run says where it starts and where it ends and nothing about where a further child sits
/// relative to it, so every widening past the fourth mark has to ask the sibling chain. Asking it
/// without a bound costs a walk of the child list per mark, which is quadratic in the width of the
/// list and is the one cost in this file that no other counter can see: the same children end up
/// marked, the same nodes are visited, and the same work comes out.
fn sibling_steps_for(order: impl Iterator<Item = usize>) -> u64 {
    let (mut document, rows) = wide_row(WIDE);
    counter::reset();
    for index in order {
        propagate::mark(document.store_mut(), rows[index], Dirty::RESTYLE);
    }
    counter::get(Counter::DirtyChildSteps)
}

/// How wide the row the sibling-step budgets are written against is.
const WIDE: usize = 8_000;

/// A deterministic order that visits every row of a [`WIDE`] row exactly once, in neither document
/// order nor its reverse: stride by a number sharing no factor with the width.
fn scattered_order() -> impl Iterator<Item = usize> {
    (0..WIDE).map(|step| step * 4_001 % WIDE)
}

#[test]
fn recording_a_marked_child_costs_a_bounded_number_of_sibling_steps() {
    let _measuring = measuring();
    // Four steps per mark, against a row eight thousand wide. An implementation that searches the
    // child list instead spends thirty-two million on the scattered order alone — the measured
    // difference is a hundred and seventeen milliseconds against a hundred and twenty microseconds.
    let ceiling = 4 * WIDE as u64;

    let forwards = sibling_steps_for(0..WIDE);
    assert!(
        (1..ceiling).contains(&forwards),
        "marking a row front to back cost {forwards} sibling steps"
    );

    let backwards = sibling_steps_for((0..WIDE).rev());
    assert!(
        (1..ceiling).contains(&backwards),
        "marking a row back to front cost {backwards} sibling steps"
    );

    let scattered = sibling_steps_for(scattered_order());
    assert!(
        scattered < ceiling,
        "marking a row in scattered order cost {scattered} sibling steps"
    );
}

/// The counter above is only a budget if the walk it bounds still services every marked child, and
/// the cheapest way to satisfy a step budget is to record nothing at all.
#[test]
fn every_child_marked_in_scattered_order_is_still_serviced() {
    let _measuring = measuring();
    let (mut document, rows) = wide_row(WIDE);
    for index in scattered_order() {
        propagate::mark(document.store_mut(), rows[index], Dirty::RESTYLE);
    }

    let root = document.document_index();
    let mut serviced = Vec::new();
    walk::walk(
        document.store_mut(),
        root,
        Dirty::RESTYLE,
        &mut |_, node| serviced.push(node),
    );
    serviced.sort();
    let mut expected = rows.clone();
    expected.sort();
    assert_eq!(serviced, expected);
}

#[test]
fn a_re_mark_during_a_walk_survives_the_unwind() {
    let _measuring = measuring();
    let (mut document, leaf) = deep_tree(1_000, 6);
    let root = document.document_index();
    propagate::mark(document.store_mut(), leaf, Dirty::RESTYLE);

    let mut once = false;
    let surviving = walk::walk(
        document.store_mut(),
        root,
        Dirty::RESTYLE,
        &mut |store, node| {
            if !core::mem::replace(&mut once, true) {
                store.core(node).dirty().mark(Dirty::RESTYLE);
            }
        },
    );
    assert!(
        surviving.contains(Dirty::RESTYLE),
        "a walk that cleared the phase on the unwind would swallow the node's own re-mark and \
         report an empty union, leaving the obligation on a node nothing leads to"
    );

    // And the obligation is genuinely reachable again, which is the half a returned flag alone does
    // not prove.
    counter::reset();
    let mut serviced = Vec::new();
    walk::walk(
        document.store_mut(),
        root,
        Dirty::RESTYLE,
        &mut |_, node| serviced.push(node),
    );
    assert_eq!(serviced, vec![leaf]);
}

/// The obligations a restyle raises are retired by the walk at the tail of it, and without that the
/// *second* hover of the same row hits the marking path's own early-out, tells no ancestor anything
/// and is silently dropped. So the first hover is not the test.
#[test]
fn hovering_the_same_row_twice_restyles_it_twice() {
    let _measuring = measuring();
    const SHEET: &str = ".row { color: rgb(1, 1, 1) } .row:hover { color: rgb(9, 0, 0) }";

    let mut table = support::rows::Rows::new(200);
    let mut engine = Engine::new(&table.document);
    engine.add_author_sheet(SHEET);
    engine.restyle(&mut table.document, None);
    edit::retire(&mut table.document);

    let row = table.rows[100];
    let hover = |document: &Document, on: bool| {
        document
            .edit(&EverythingMatters, |batch| {
                batch.set_state(row, UiState::HOVER, on);
            })
            .expect("the document is not poisoned");
    };

    hover(&table.document, true);
    engine.restyle(&mut table.document, None);
    edit::retire(&mut table.document);
    assert_eq!(support::read::color(&table.document, row), (9, 0, 0));

    hover(&table.document, false);
    engine.restyle(&mut table.document, None);
    edit::retire(&mut table.document);
    assert_eq!(support::read::color(&table.document, row), (1, 1, 1));

    hover(&table.document, true);
    let pass = engine.restyle(&mut table.document, None);
    assert!(pass.visited.contains(&row.get()));
    assert_eq!(
        support::read::color(&table.document, row),
        (9, 0, 0),
        "the second hover of the same row has to take effect, not merely the first"
    );

    // The idle frame after it. The retirement that follows the restyle drains the path the hover
    // marked; the frame after that has nothing to do and no walk enters a single node.
    edit::retire(&mut table.document);
    counter::reset();
    edit::retire(&mut table.document);
    assert_eq!(counter::get(Counter::NodesVisited), 0);
    assert_eq!(counter::get(Counter::DirtyWalkSteps), 0);
    let idle = engine.restyle(&mut table.document, None);
    assert_eq!(idle.restyled, 0);
}

/// One marked child of a wide row costs one probe with the record and one per child without it, and
/// no counter but this one can tell the two apart.
#[test]
fn one_marked_child_of_a_wide_row_costs_one_probe() {
    let _measuring = measuring();
    let (mut document, rows) = wide_row(10_000);
    propagate::mark(document.store_mut(), rows[7_777], Dirty::RESTYLE);

    counter::reset();
    let root = document.document_index();
    walk::walk(document.store_mut(), root, Dirty::RESTYLE, &mut |_, _| {});
    assert_eq!(counter::get(Counter::NodesVisited), 3);
    assert_eq!(counter::get(Counter::DirtyWalkSteps), 2);
}

/// A change to a child list under a parent no selector depends on records nothing, so the marking
/// it causes is the parent, the changed node and the path between them — not the child list.
#[test]
fn appending_a_row_under_an_unwatched_parent_marks_a_constant_number_of_nodes() {
    let _measuring = measuring();
    let (mut document, rows) = wide_row(10_000);
    let container = document
        .store()
        .core(rows[0])
        .parent()
        .expect("the rows have a parent");
    edit::retire(&mut document);

    document
        .edit(&EverythingMatters, |batch| {
            let fresh = batch.create_element(ElementName::new("li"));
            batch.set_classes(fresh, &[ClassName::new("row")]);
            batch.insert_before(container, fresh, Some(rows[0]));
        })
        .expect("the document is not poisoned");

    counter::reset();
    let root = document.document_index();
    let mut serviced = 0;
    walk::walk(document.store_mut(), root, Dirty::all(), &mut |_, _| {
        serviced += 1
    });
    assert!(
        counter::get(Counter::NodesVisited) < 8,
        "visited {} nodes for one insertion under a parent nothing depends on",
        counter::get(Counter::NodesVisited)
    );
    assert!(counter::get(Counter::DirtyWalkSteps) < 8);
}
