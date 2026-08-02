//! What building a box tree and laying it out costs, in the numbers a budget is written in.

mod support;

use std::sync::{Mutex, MutexGuard, PoisonError};

use support::{Element, Fixture, lay_out, measurer};
use zgui_profile::{Counter, counter};

/// The counter block is process-wide, so cases that write to it take turns.
fn exclusive() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs `body` with the counters reset, and reports the three this crate produces.
///
/// The caller holds the lock for its whole case, not just for this call: work done *outside* a
/// measurement still moves the process-wide block, so a case that only locked while measuring would
/// read another case's boxes.
fn measure(body: impl FnOnce()) -> (u64, u64, u64) {
    counter::reset();
    body();
    (
        counter::get(Counter::BoxesRebuilt),
        counter::get(Counter::NodesRelaidOut),
        counter::get(Counter::LayoutReachedRoot),
    )
}

#[test]
fn building_a_tree_counts_one_box_per_box_it_built() {
    let _guard = exclusive();
    // Three elements, one text run each, and one anonymous wrapper per element that holds a run:
    // six boxes. A counter that moved by the number of *elements* would look right on a document
    // with no anonymous boxes in it, which is the document nobody has.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("a").text("one"),
            Element::new("b").text("two"),
        ]),
        "root { display: block; width: 200px }
         a { display: block }
         b { display: block }",
    );
    let (built, _, _) = measure(|| {
        let store = fixture.box_tree();
        assert!(!store.is_empty());
    });
    assert_eq!(
        built, 5,
        "three element boxes and two text runs were built, and the anonymous wrappers are not \
         counted because no element was rebuilt for them"
    );
}

#[test]
fn a_layout_from_the_root_counts_once_and_relays_out_every_box() {
    let _guard = exclusive();
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("a").text("one"),
            Element::new("b").text("two"),
        ]),
        "root { display: block; width: 200px }
         a { display: block }
         b { display: block }",
    );
    let mut store = fixture.box_tree();
    assert_eq!(store.len(), 7, "the fixture's shape changed");
    let (_, relaid, reached_root) = measure(|| {
        let mut again = measurer();
        lay_out(&mut store, &mut again, 200.0, 300.0);
    });
    assert_eq!(reached_root, 1, "one pass, entered once, at the root");
    // Five of the seven boxes are laid out: the root, the two block-level children and the
    // anonymous wrapper inside each. The two text runs are not, because the wrapper around them is
    // a leaf whose size comes from a measurement rather than from an algorithm.
    assert_eq!(relaid, 5, "{relaid} boxes were laid out rather than five");
}

#[test]
fn a_second_pass_over_an_untouched_tree_reaches_the_root_again_and_recomputes_nothing() {
    let _guard = exclusive();
    // The control that makes the counter above mean something: if `NodesRelaidOut` moved by the
    // same amount whatever the cache held, it would be counting boxes rather than work.
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("a").text("one")]),
        "root { display: block; width: 200px }
         a { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 200.0, 300.0);

    let (_, relaid, reached_root) = measure(|| {
        let mut again = measurer();
        lay_out(&mut store, &mut again, 200.0, 300.0);
    });
    assert_eq!(reached_root, 1);
    assert_eq!(
        relaid, 0,
        "a repeated pass at the same size recomputed {relaid} boxes"
    );
}

#[test]
fn invalidating_a_text_run_reaches_the_box_that_measured_it() {
    let _guard = exclusive();
    // A run of text is never laid out on its own — the anonymous box around it is the leaf, and it
    // is that box's cache that holds the measurement. So marking the run has to reach it, and then
    // the ancestors above. Both halves fail silently: a run whose parent is the block *above* the
    // wrapper walks straight past the box that holds the answer, and a walk that treats "no result
    // yet" as "already invalidated" stops on the run itself and marks nothing at all.
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("a").text("one")]),
        "root { display: block; width: 200px }
         a { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 200.0, 300.0);

    let root = store.root().expect("a root");
    let block = store.node(root).children[0];
    let wrapper = store.node(block).children[0];
    let run = store.node(wrapper).children[0];
    assert!(
        !zgui_layout::tree::dirty::is_dirty(&store, wrapper),
        "the wrapper holds a measurement before anything is marked"
    );

    let marked = zgui_layout::tree::dirty::mark_dirty(&mut store, run);
    assert_eq!(marked, 4, "the run, the wrapper, the block and the root");
    for key in [wrapper, block, root] {
        assert!(
            zgui_layout::tree::dirty::is_dirty(&store, key),
            "a box above the run kept its result"
        );
    }

    // And the re-layout actually re-measures: a wrapper left warm would answer from its cache.
    let (_, relaid, _) = measure(|| {
        let mut again = measurer();
        lay_out(&mut store, &mut again, 200.0, 300.0);
    });
    assert_eq!(relaid, 3, "the root, the block and the wrapper");
}

#[test]
fn invalidating_one_box_costs_that_box_and_its_ancestors_and_no_more() {
    let _guard = exclusive();
    // Marking is `O(n + depth)` because it stops at the first already-invalid ancestor. A second
    // mark inside the same subtree therefore costs one step, not another walk to the root.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("a").children(vec![Element::new("b").text("one")]),
        ]),
        "root { display: block; width: 200px }
         a { display: block }
         b { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 200.0, 300.0);

    let root = store.root().expect("a root");
    let a = store.node(root).children[0];
    let b = store.node(a).children[0];
    let first = zgui_layout::tree::dirty::mark_dirty(&mut store, b);
    assert_eq!(first, 3, "the box and its two ancestors");
    let second = zgui_layout::tree::dirty::mark_dirty(&mut store, b);
    assert_eq!(second, 0, "everything above it is already invalid");
    let sibling = zgui_layout::tree::dirty::mark_dirty(&mut store, a);
    assert_eq!(sibling, 0, "and so is it");
}

#[test]
fn a_flex_row_shapes_once_per_paragraph_and_breaks_once_per_width() {
    let _guard = exclusive();
    // The same criterion the protocol tests assert against the shaper's own view, read here from
    // the process-wide counters a budget is written in. Both are needed: the shaper's numbers say
    // what it did, and these say what the framework told anyone watching.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("item").text("one two"),
            Element::new("item").text("three four"),
        ]),
        "root { display: flex; width: 400px }
         item { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    counter::reset();
    lay_out(&mut store, &mut content, 400.0, 300.0);
    let shaped = counter::get(Counter::TextShaped);
    let rebroken = counter::get(Counter::TextRebroken);

    assert_eq!(shaped, 2, "two paragraphs, two shapes");
    assert_eq!(
        rebroken,
        u64::from(content.shaper().breaks),
        "the counter and the shaper disagree about how many passes ran"
    );
    assert!(
        rebroken < 8,
        "{rebroken} breaking passes for two paragraphs: the intrinsic probes are breaking"
    );

    // Laying the same tree out again at the same size costs nothing at all.
    let root = store.root().expect("a root");
    zgui_layout::tree::dirty::mark_dirty(&mut store, root);
    counter::reset();
    lay_out(&mut store, &mut content, 400.0, 300.0);
    assert_eq!(
        counter::get(Counter::TextShaped),
        0,
        "nothing was re-shaped"
    );
}
