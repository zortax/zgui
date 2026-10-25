//! What a keyed list reordering its rows costs the style engine: nothing below the rows.
//!
//! A row moved among its own siblings keeps the parent it inherits from, so nothing under it can
//! cascade differently. The only style work a reorder owes is the rows' own re-match — sibling
//! combinators and `:nth-*` can see the new positions — and that is proportional to the rows, never
//! to everything they contain.

mod support;

use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_view::{BuildCx, ForProps, IntoView, NodeRef, View};

const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   .row { display: flex; height: 10px }
                   .c { display: block; width: 40px }";

const ROWS: u32 = 300;

#[test]
fn reordering_rows_recascades_nothing_below_them() {
    let _turn = zgui_profile::counter::exclusive();
    let rows = RwSignal::new((0..ROWS).collect::<Vec<u32>>());
    let first = NodeRef::new();
    let mut harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    ForProps::builder()
                        .each(move || rows.get())
                        .key(|row: &u32| *row)
                        .children(move |row: u32| {
                            let mut built = zgui_elements::row().class("row");
                            if row == 0 {
                                built = built.node_ref(first);
                            }
                            for cell in 0..6 {
                                built = built.child(
                                    zgui_elements::text()
                                        .class("c")
                                        .child(format!("{row}-{cell}")),
                                );
                            }
                            built
                        })
                        .build()
                        .render(),
                )
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    assert_eq!(
        top_of(&harness, first),
        0.0,
        "the first row starts at the top"
    );

    let before = zgui_profile::counter::snapshot();
    rows.set((0..ROWS).rev().collect());
    harness.settle(8);
    let moved = before.delta(&zgui_profile::counter::snapshot());
    assert_eq!(
        top_of(&harness, first),
        10.0 * (ROWS - 1) as f32,
        "the row that was first is not laid out last after the reverse"
    );

    if !zgui_profile::COUNTERS_ENABLED {
        return;
    }
    assert_eq!(
        moved.elements_recascaded, 0,
        "a reorder within one parent recascaded {} elements below the rows",
        moved.elements_recascaded
    );
    assert!(
        moved.elements_restyled <= u64::from(ROWS) + 2,
        "a reorder restyled {} elements, more than the rows that moved",
        moved.elements_restyled
    );

    // The accessibility tree hears about the rows and their parent, and about none of the cells:
    // a cell is measured from its row, and it did not move within it.
    let update = harness
        .platform()
        .offscreens()
        .first()
        .expect("a surface was created")
        .last_a11y_update()
        .expect("the reorder published an accessibility update");
    assert!(
        update.nodes.len() <= ROWS as usize + 2,
        "a reorder of {ROWS} rows re-sent {} accessibility nodes",
        update.nodes.len()
    );
    let tree = support::a11y_tree(&harness);
    let state = tree.state();
    let id = first
        .get_untracked()
        .expect("the first row is bound")
        .as_u64();
    let row = state
        .node_by_tree_local_id(zgui_a11y::NodeId(id), zgui_a11y::TreeId::ROOT)
        .expect("the first row is in the tree");
    let composed = row.bounding_box().expect("the row has a rectangle");
    assert_eq!(composed.y0, 10.0 * f64::from(ROWS - 1), "{composed:?}");
    let cell = row.children().next().expect("the row has cells");
    let composed = cell.bounding_box().expect("the cell has a rectangle");
    assert_eq!(composed.y0, 10.0 * f64::from(ROWS - 1), "{composed:?}");
    harness.shut_down();
}

/// Where the element `handle` names is drawn, as the top of its first fragment.
fn top_of(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    handle: NodeRef,
) -> f32 {
    let window = &harness.app().windows()[0];
    let node = handle.get().expect("the row is mounted");
    let index = window.dom().live_index_of(node).expect("the row is live");
    let key = window.document().borrow().store().key_of(index);
    let layout = window.layout().borrow();
    let box_ = layout.boxes_of(key)[0];
    let frag = layout.fragments_of_box(box_)[0];
    let fragment = layout.fragment(frag).expect("a live fragment");
    fragment
        .on_device(fragment.border_box, &window.placements())
        .origin
        .y
        .0
}
