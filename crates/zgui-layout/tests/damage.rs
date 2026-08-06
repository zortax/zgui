//! What the fragment pass reports as needing to be redrawn.

mod support;

use support::{Element, Fixture, fragments, lay_out, measurer};
use zgui_bits::{DamageSet, Dirty};
use zgui_geom::{Device, DevicePx, Rect};
use zgui_layout::fragment::diff::{Everything, FrameDirty};
use zgui_layout::{BoxKey, LayoutStore};

/// The box the second child element generated.
fn second_child(store: &LayoutStore) -> BoxKey {
    let root = store.root().expect("a root");
    store.node(root).children[1]
}

/// Whether the damage set covers every pixel of `rect`.
fn covers(damage: &DamageSet, rect: Rect<DevicePx, Device>) -> bool {
    let pixels = zgui_layout::fragment::diff::pixels(rect);
    damage.rects().iter().any(|held| held.contains_rect(pixels))
}

/// A dirty answer marking one element and nothing else.
struct OnlyThis {
    /// The element that owes work.
    node: Option<zgui_dom::NodeKey>,
    /// What it owes.
    bits: Dirty,
    /// What was marked while the pass ran.
    marked: Vec<(Option<zgui_dom::NodeKey>, Dirty)>,
}

impl FrameDirty for OnlyThis {
    fn own(&self, node: Option<zgui_dom::NodeKey>) -> Dirty {
        if node.is_some() && node == self.node {
            self.bits
        } else {
            Dirty::empty()
        }
    }

    fn subtree(&self, _node: Option<zgui_dom::NodeKey>) -> Dirty {
        // Everything is descended through, so the case is about what the *compare* decides and
        // never about a subtree the walk skipped.
        Dirty::all()
    }

    fn mark(&mut self, node: Option<zgui_dom::NodeKey>, bits: Dirty) {
        self.marked.push((node, bits));
    }

    fn retire(&mut self, _node: Option<zgui_dom::NodeKey>, _phase: Dirty) {
        // The marks here are the fixture's own field rather than a document's, and every case runs
        // exactly one pass over them. Retiring would erase the record of what the pass marked,
        // which is the thing several of these assert on.
    }
}

#[test]
fn a_paint_only_change_still_damages_the_fragment_it_repainted() {
    // The commonest frame in a component library: a hover changes a background colour. Nothing
    // moves, so the geometry compare says the fragment is identical — and without absorbing its
    // ink on the strength of the node's own `REPAINT` bit the damage set would be empty, the paint
    // stage would be handed no rectangle, and the button would never change colour.
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("a"), Element::new("b")]),
        "root { display: block; width: 200px }
         a { display: block; height: 30px }
         b { display: block; height: 30px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 200.0, 200.0);

    let target = second_child(&store);
    let ink = {
        let frag = store.fragments_of_box(target)[0];
        store.fragment(frag).expect("live").ink
    };
    let node = store.node(target).source;

    frame.damage = DamageSet::new();
    let root = store.root().expect("a root");
    let mut dirty = OnlyThis {
        node,
        bits: Dirty::REPAINT,
        marked: Vec::new(),
    };
    fragments(&mut frame, &mut store, root, &mut dirty);

    assert!(!frame.damage.is_empty(), "a repaint damaged nothing at all");
    assert!(
        covers(&frame.damage, ink),
        "the repainted piece is not in it"
    );
}

#[test]
fn a_fragment_that_only_moved_is_marked_for_repositioning_and_damages_both_places() {
    // Scrolling is the case exactly: every piece inside the scrollport keeps its size and its
    // shape and lands somewhere else. The paint stage services that by offsetting what it recorded
    // rather than producing it again, and both rectangles have to be redrawn — the one being
    // vacated as much as the one being filled.
    //
    // The two are not cut to the same thing, and the asymmetry is deliberate. Where the piece has
    // *arrived* is cut to what its clip chain admits, because a primitive the chain admits nothing
    // of is refused before it is drawn — this row is four hundred pixels tall in a hundred-pixel
    // port, so three quarters of it is pixels no frame will ever put anything into. Where it was
    // is not cut at all: the chain it named belongs to a frame that is gone, and cutting a vacated
    // rectangle to a region that has since moved is how a scrolled row's old pixels get left on
    // the screen.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("port").children(vec![Element::new("row")]),
        ]),
        "root { display: block; width: 300px }
         port { display: block; height: 100px; overflow: scroll }
         row { display: block; height: 400px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 300.0, 300.0);

    let root = store.root().expect("a root");
    let port = store.node(root).children[0];
    let row = store.node(port).children[0];
    let node = store.node(row).source;
    let before = {
        let frag = store.fragments_of_box(row)[0];
        store.fragment(frag).expect("live").ink
    };

    frame.damage = DamageSet::new();
    frame.scroll.scroll_to(
        &store,
        store
            .node(port)
            .source
            .expect("the scrollport is an element"),
        zgui_geom::Point::new(DevicePx(0.0), DevicePx(40.0)),
    );
    let mut dirty = OnlyThis {
        node,
        bits: Dirty::empty(),
        marked: Vec::new(),
    };
    fragments(&mut frame, &mut store, root, &mut dirty);

    let after = {
        let frag = store.fragments_of_box(row)[0];
        store.fragment(frag).expect("live").ink
    };
    assert_eq!(after.origin.y.0, before.origin.y.0 - 40.0);
    assert_eq!(after.size, before.size, "it moved and did nothing else");
    assert!(
        dirty
            .marked
            .iter()
            .any(|(marked, bits)| *marked == node && bits.contains(Dirty::REPOSITION)),
        "the moved row was not marked for repositioning: {:?}",
        dirty.marked
    );
    let port_rect = zgui_layout::scroll_region::region_of(&store, port)
        .expect("the port scrolls")
        .scrollport;
    let arrived = after
        .intersection(port_rect)
        .expect("the row is still in the port");
    assert!(covers(&frame.damage, before), "the rectangle it left");
    assert!(
        covers(&frame.damage, arrived),
        "and the part of the one it arrived at that it can actually draw in",
    );
    assert!(
        !covers(&frame.damage, after),
        "but not the three hundred pixels of it that lie outside the port, which nothing draws",
    );
}

#[test]
fn a_box_that_stops_producing_a_piece_damages_where_that_piece_was() {
    // A paragraph that loses a line leaves the line's pixels behind. Nothing downstream can find
    // them: the fragment they belonged to no longer exists, so it is nobody's ink and no later
    // stage could recover the rectangle.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("para").text("alpha bravo delta gamma kappa sigma omega"),
        ]),
        "root { display: block }
         para { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 200.0, 400.0);

    let root = store.root().expect("a root");
    let para = store.node(root).children[0];
    let inline = store.node(para).children[0];
    let lines = store.fragments_of_box(inline).len();
    assert!(lines > 2, "the paragraph wrapped");
    let last = *store.fragments_of_box(inline).last().expect("a line");
    let vacated = store.fragment(last).expect("live").ink;

    // Laid out much wider, the paragraph fits on fewer lines and the last of them ceases to exist.
    frame.damage = DamageSet::new();
    {
        let mut tree = zgui_layout::tree::LayoutTree::new(
            &mut store,
            &mut content,
            zgui_layout::DeviceStyle::default(),
        );
        assert!(tree.layout_root(taffy::Size {
            width: 2000.0,
            height: 400.0
        }));
    }
    fragments(&mut frame, &mut store, root, &mut Everything);

    assert!(
        store.fragments_of_box(inline).len() < lines,
        "the paragraph did lose a line"
    );
    assert!(
        covers(&frame.damage, vacated),
        "the rectangle the lost line occupied is not in the damage"
    );
    assert!(
        !store.fragments_of_box(inline).contains(&last),
        "the box still lists a line it no longer draws"
    );
    // Records outlive the pass that dropped them and stop resolving when the frame is recycled,
    // which is what lets everything in the frame still read the geometry that was.
    store.recycle();
    assert!(store.fragment(last).is_none());
}
