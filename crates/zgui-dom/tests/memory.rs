//! What a node costs, measured rather than asserted.
//!
//! Two numbers decide the shape of the whole store, and both are easy to be wrong about by an order
//! of magnitude if they are reasoned about instead of measured.
//!
//! The first is the fixed cost of *having* a node: the record, its arena slot, its entry in the key
//! table and the class pool. The second is what the columns add, and it is the one the sparse
//! layout exists for — a dense table costs its value type on every node in the document whether or
//! not the node participates, so a handful of dense columns over a hundred thousand nodes is tens of
//! megabytes before anything has been stored in any of them.

use zgui_dom::{Document, EverythingMatters, NodeIndex, NodeKind};
use zgui_interned::{AttrName, ClassName, ElementName};
use zgui_vocab::SharedString;

/// How many nodes the budget is written against.
const NODES: usize = 100_000;

/// What a page of a sparse column costs: one thousand entries plus the pointer that finds it.
fn sparse_bytes<V>(pages: usize) -> usize {
    pages * (zgui_arena::PAGE_LEN * size_of::<V>() + size_of::<usize>())
}

/// A wide, shallow document of `count` element children under one root.
fn document_of(count: usize) -> (Document, Vec<NodeIndex>) {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    let nodes = (0..count)
        .map(|_| document.append(root, NodeKind::Element, ElementName::new("item")))
        .collect();
    (document, nodes)
}

#[test]
fn a_hundred_thousand_nodes_with_three_populated_columns_stay_under_six_megabytes() {
    let (mut document, nodes) = document_of(NODES);

    // Three columns, populated on every node — attributes, text and semantics. Every page of each
    // is therefore allocated, which is the worst case the sparse layout has to survive.
    for (index, node) in nodes.iter().enumerate() {
        let key = document.store().key_of(*node);
        let mut attrs = zgui_dom::side::AttrMap::new();
        attrs.set(AttrName::new("role"), SharedString::from("row"));
        *document.store_mut().columns_mut().attrs.get_mut(key) = Some(Box::new(attrs));
        *document.store_mut().columns_mut().text.get_mut(key) = Some(format!("{index}").into());
        *document.store_mut().columns_mut().semantics.get_mut(key) = Some(Box::new(
            zgui_vocab::Semantics::from(zgui_vocab::A11y::new(zgui_vocab::Role::Row)),
        ));
    }

    let columns = document.store().columns();
    let bytes = sparse_bytes::<Option<Box<zgui_dom::side::AttrMap>>>(columns.attrs.pages())
        + sparse_bytes::<Option<Box<str>>>(columns.text.pages())
        + sparse_bytes::<zgui_dom::side::SemanticsSlot>(columns.semantics.pages());

    assert!(
        bytes < 6 * 1024 * 1024,
        "three populated columns over {NODES} nodes cost {bytes} bytes of column storage"
    );
    assert_eq!(
        columns.allocated_pages(),
        3 * NODES.div_ceil(zgui_arena::PAGE_LEN),
        "and the other seven sparse columns allocated nothing"
    );
}

#[test]
fn one_dense_column_of_the_widest_sparse_value_would_cost_more_than_all_ten_sparse_ones() {
    // Why the split is where it is, stated as arithmetic rather than as a claim. A dense table pays
    // its value type on every slot in the document; a sparse one pays a pointer per thousand slots
    // for the runs nothing was written into.
    let dense = NODES * size_of::<zgui_dom::side::ObservationSlots>();
    let sparse_when_empty = 10 * size_of::<usize>() * NODES.div_ceil(zgui_arena::PAGE_LEN);
    assert!(
        dense > sparse_when_empty * 20,
        "one dense column is {dense} bytes against {sparse_when_empty} for ten empty sparse ones"
    );
}

#[test]
fn the_columns_nothing_was_written_to_cost_nothing_at_all() {
    let (document, _) = document_of(NODES);
    assert_eq!(
        document.store().columns().allocated_pages(),
        0,
        "a document that has only been built allocates no column page"
    );
}

#[test]
fn the_fixed_cost_of_a_node_is_recorded_rather_than_assumed() {
    let (mut document, nodes) = document_of(10_000);
    for node in &nodes {
        document.set_classes(*node, &[ClassName::new("item")]);
    }

    let per_node = document.store().bytes_per_node();
    // The record dominates, and the arena rounds up to whole blocks, so the figure sits a little
    // above the record's own size. What this pins is the *shape*: the fixed cost is the record plus
    // a small constant, not the record plus a dozen dense tables.
    assert!(
        per_node < (size_of::<zgui_dom::NodeInner>() + 32) as f64,
        "a node costs {per_node} bytes of record, slot, key and class pool"
    );
    assert!(per_node > size_of::<zgui_dom::NodeInner>() as f64 * 0.9);
}

#[test]
fn interning_the_same_class_run_keeps_the_pool_flat() {
    // The pool is append-only, so a class written on every node of a large document would be the
    // one place a wide document could grow without bound. Reuse is what stops it.
    let (mut document, nodes) = document_of(10_000);
    for node in &nodes {
        document.set_classes(*node, &[ClassName::new("item"), ClassName::new("striped")]);
    }
    assert_eq!(
        document.store().class_pool().len(),
        2,
        "ten thousand nodes sharing two class names hold two entries between them"
    );
}

/// How many rows one mount puts in the document.
const ROWS: usize = 100;

/// How many times the list is mounted and unmounted again.
const ROUNDS: usize = 200;

/// Mounts `ROWS` rows under `root`, each with an attribute and a text child, and returns the roots.
fn mount(document: &Document, root: NodeIndex) -> Vec<NodeIndex> {
    document
        .edit(&EverythingMatters, |batch| {
            (0..ROWS)
                .map(|index| {
                    let row = batch.create_element(ElementName::new("li"));
                    batch.set_classes(row, &[ClassName::new("row")]);
                    batch.set_attribute(
                        row,
                        AttrName::new("role"),
                        Some(SharedString::from("listitem")),
                    );
                    let text = batch.create_text("");
                    batch.set_text(text, &format!("row {index}"));
                    batch.insert_before(row, text, None);
                    batch.insert_before(root, row, None);
                    row
                })
                .collect()
        })
        .expect("the document is not poisoned")
}

/// Takes `rows` back out again.
fn unmount(document: &Document, rows: &[NodeIndex]) {
    document
        .edit(&EverythingMatters, |batch| {
            for row in rows {
                batch.remove(*row);
            }
        })
        .expect("the document is not poisoned");
}

/// A document that mounts and unmounts a list for as long as an application runs has to come back
/// to where it started. Nothing about a removal is observable from the outside — the nodes are gone
/// from the tree either way — so a removal that never drops anything looks exactly like one that
/// does, until the process runs out of memory some hours in.
#[test]
fn mounting_and_unmounting_a_list_returns_the_arena_to_its_baseline() {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    let baseline = document.len();
    assert_eq!(baseline, 2, "the document node and the root");

    // The control, without which every assertion below could be satisfied by a mount that mounts
    // nothing: one round really does put two hundred nodes and their column rows in the document.
    let rows = mount(&document, root);
    assert_eq!(document.len(), baseline + 2 * ROWS);
    let mounted_pages = document.store().columns().allocated_pages();
    assert!(mounted_pages > 0, "a mounted row writes to a column");
    unmount(&document, &rows);
    assert_eq!(
        document.len(),
        baseline + 2 * ROWS,
        "a removed node is still readable until the frame it was removed in ends"
    );
    zgui_dom::arena::end_frame(&mut document);
    assert_eq!(document.len(), baseline);

    let high_water = document.store().slot_count();
    for _ in 0..ROUNDS {
        let rows = mount(&document, root);
        unmount(&document, &rows);
        zgui_dom::arena::end_frame(&mut document);
    }

    assert_eq!(
        document.len(),
        baseline,
        "the document holds nodes from rounds that ended"
    );
    assert_eq!(
        document.store().slot_count(),
        high_water,
        "every round after the first one reused the slots the round before it gave back"
    );
    assert_eq!(
        document.store().columns().allocated_pages(),
        0,
        "the attribute and text rows of {ROUNDS} rounds of rows are still allocated"
    );
    assert_eq!(
        document.store().class_pool().len(),
        1,
        "the rows shared one class name between them"
    );
}

/// The other half of the deferral, from the outside: a slot given back at the end of a frame is
/// reused by the next one, and the key that named its previous occupant does not follow it there.
#[test]
fn a_slot_reused_after_a_frame_does_not_answer_to_its_previous_occupants_key() {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    let gone = document.append(root, NodeKind::Element, ElementName::new("li"));
    let key = document.store().key_of(gone);

    document
        .edit(&EverythingMatters, |batch| batch.remove(gone))
        .expect("the document is not poisoned");
    assert!(
        document.store().get(key).is_some(),
        "the frame that removed it has not ended"
    );

    zgui_dom::arena::end_frame(&mut document);
    assert!(document.store().get(key).is_none());
    assert!(document.store().try_core(gone).is_none());

    let fresh = document.append(root, NodeKind::Element, ElementName::new("li"));
    assert_eq!(fresh, gone, "the slot number came back");
    assert_ne!(
        document.store().key_of(fresh),
        key,
        "and the key that named the slot's last occupant is not the key that names this one"
    );
    assert!(document.store().get(key).is_none());
}
