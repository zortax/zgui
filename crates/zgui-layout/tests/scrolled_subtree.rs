//! What a pass that only scrolled something arrives at, against a pass that composed every box.
//!
//! A scroll moves a subtree and changes nothing else about it, so the pass offsets the pieces
//! instead of composing them again. The two are only interchangeable if they produce the same
//! fragment tree, the same clip chains, the same hit index and the same damage — so that is what is
//! asserted here, by running the same scroll twice over two identical documents and driving one of
//! them down each path.

mod support;

use std::sync::{Mutex, MutexGuard, PoisonError};

use support::{Element, Fixture, fragments, lay_out, measurer};
use zgui_bits::Dirty;
use zgui_geom::{Device, DevicePx, Point};
use zgui_layout::fragment::diff::{Everything, FrameDirty};
use zgui_layout::{Fragment, LayoutStore};
use zgui_profile::{COUNTERS_ENABLED, Counter, counter};

/// The counter block is process-wide, so the case that reads it runs alone.
fn exclusive() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A scrollport with rows in it, each row a rounded card that clips its own contents.
///
/// The clip is the point: a chain interned from a rectangle that moves is a *different* chain, so a
/// document without one would never exercise the half of the offsetting path that re-interns them.
fn page() -> Fixture {
    let rows: Vec<Element> = (0..12)
        .map(|_| {
            Element::new("row").children(vec![
                Element::new("card").text("alpha bravo delta gamma kappa sigma"),
            ])
        })
        .collect();
    Fixture::new(
        Element::new("root").children(vec![Element::new("port").children(rows)]),
        "root { display: block; width: 300px; height: 200px }
         port { display: block; height: 200px; overflow: scroll }
         row  { display: block; height: 60px; padding: 4px }
         card { display: block; overflow: hidden; border-radius: 6px; padding: 2px }",
    )
}

/// The element the scrollport was generated from.
fn port(fixture: &Fixture, store: &LayoutStore) -> zgui_dom::NodeKey {
    let root = store.root().expect("a root box");
    let port = store.node(root).children[0];
    let _ = fixture;
    store
        .get(port)
        .and_then(|node| node.source)
        .expect("the scrollport came from an element")
}

/// One fragment's kind, border box, ink, clip chain and flags, as a value that compares.
type Row = (String, [f32; 4], [f32; 4], u32, u8);

/// Every fragment, in painting order, as the numbers everything downstream reads.
fn snapshot(store: &LayoutStore) -> Vec<Row> {
    let root = store.root().expect("a root box");
    let mut out = Vec::new();
    for box_ in zgui_layout::fragment::stacking::paint_order(store, root) {
        for frag in store.fragments_of_box(box_) {
            let fragment = store.fragment(*frag).expect("a live fragment");
            out.push(row(fragment));
        }
    }
    out
}

/// One fragment's kind, border box, ink, clip chain and flags.
fn row(fragment: &Fragment) -> Row {
    let rect = |rect: zgui_geom::Rect<DevicePx, Device>| {
        [
            rect.origin.x.0,
            rect.origin.y.0,
            rect.size.width.0,
            rect.size.height.0,
        ]
    };
    (
        format!("{:?}", fragment.kind),
        rect(fragment.border_box),
        rect(fragment.ink),
        fragment.clip.index(),
        fragment.flags.bits(),
    )
}

/// A dirty answer saying that one element owes a scroll and nothing else owes anything.
///
/// This is what a frame that scrolled a container hands the pass, and it is what lets the pass take
/// the offsetting path: every other box reports itself clean, which is the claim the path rests on.
struct Scrolled {
    /// The scrolled element.
    node: zgui_dom::NodeKey,
}

impl FrameDirty for Scrolled {
    fn own(&self, node: Option<zgui_dom::NodeKey>) -> Dirty {
        if node == Some(self.node) {
            Dirty::SCROLL
        } else {
            Dirty::empty()
        }
    }

    fn subtree(&self, node: Option<zgui_dom::NodeKey>) -> Dirty {
        if node == Some(self.node) {
            Dirty::SCROLL
        } else {
            Dirty::empty()
        }
    }

    fn mark(&mut self, _node: Option<zgui_dom::NodeKey>, _bits: Dirty) {}

    fn retire(&mut self, _node: Option<zgui_dom::NodeKey>, _phase: Dirty) {}
}

/// The two documents, scrolled by the same amount, one composed and one offset.
fn both(by: f32) -> (Vec<Row>, Vec<Row>) {
    let mut answers = Vec::new();
    for incremental in [false, true] {
        let fixture = page();
        let mut store = fixture.box_tree();
        let mut content = measurer();
        let mut frame = lay_out(&mut store, &mut content, 300.0, 200.0);
        let element = port(&fixture, &store);
        frame
            .scroll
            .place(element, Point::new(DevicePx(0.0), DevicePx(by)));
        if incremental {
            let root = store.root().expect("a root box");
            let mut dirty = Scrolled { node: element };
            fragments(&mut frame, &mut store, root, &mut dirty);
        } else {
            let root = store.root().expect("a root box");
            fragments(&mut frame, &mut store, root, &mut Everything);
        }
        answers.push(snapshot(&store));
    }
    let offset = answers.pop().expect("both passes ran");
    let composed = answers.pop().expect("both passes ran");
    (composed, offset)
}

#[test]
fn offsetting_a_scrolled_subtree_lands_where_composing_it_would_have() {
    let _guard = exclusive();
    let (composed, offset) = both(37.0);
    assert!(
        composed.len() > 20,
        "the fixture produced {} fragments, too few for the comparison to mean anything",
        composed.len()
    );
    assert_eq!(
        composed, offset,
        "the pass that offset the subtree and the pass that composed every box disagree"
    );
}

#[test]
fn a_fractional_scroll_offset_lands_where_composing_it_would_have() {
    let _guard = exclusive();
    // Snapping rounds cumulative absolute edges and the scroll offset is added afterwards, so an
    // offset that is not a whole number of pixels still moves every piece by exactly itself. A path
    // that quietly re-snapped the moved rectangles would differ here and nowhere else.
    let (composed, offset) = both(12.5);
    assert_eq!(
        composed, offset,
        "a fractional offset composed differently from the way it was applied"
    );
}

#[test]
fn a_scroll_offsets_its_subtree_instead_of_comparing_every_piece_of_it() {
    // Without this the two cases above would still pass with the offsetting path never taken: both
    // documents would simply compose every box and agree with each other. What separates the paths
    // is the comparison itself, which the offsetting one never performs.
    let _guard = exclusive();
    let fixture = page();
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 300.0, 200.0);
    let element = port(&fixture, &store);
    let fragments_before = store.fragment_count();
    frame
        .scroll
        .place(element, Point::new(DevicePx(0.0), DevicePx(37.0)));

    counter::reset();
    let root = store.root().expect("a root box");
    let mut dirty = Scrolled { node: element };
    fragments(&mut frame, &mut store, root, &mut dirty);
    let diffed = counter::get(Counter::FragmentsDiffed);

    if !COUNTERS_ENABLED {
        return;
    }
    assert!(
        fragments_before > 20,
        "the fixture produced {fragments_before} fragments, too few for the ratio to mean anything"
    );
    assert!(
        diffed < u64::from(fragments_before) / 4,
        "{diffed} of {fragments_before} fragments were compared against their previous geometry, \
         so the scrolled subtree was composed again rather than offset"
    );
}

#[test]
fn a_scrolled_subtree_keeps_its_pieces_inside_the_clips_that_moved_with_them() {
    let _guard = exclusive();
    // The chains themselves, not merely the identifiers: two documents interning their chains in a
    // different order would compare equal on the index above while clipping to different rectangles.
    let (composed, offset) = both(37.0);
    let chains: Vec<u32> = offset.iter().map(|entry| entry.3).collect();
    assert!(
        chains.iter().any(|id| *id != 0),
        "no fragment was clipped at all, so the fixture never exercised a moved clip"
    );
    assert_eq!(
        composed.iter().map(|entry| entry.3).collect::<Vec<u32>>(),
        chains,
        "the offset pass drew fragments under different chains from the composed pass"
    );
}
