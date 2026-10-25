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
        // The repainted box carries a background, because the damage is owed for pixels: a box
        // that paints nothing owes none, which the companion case below pins.
        "root { display: block; width: 200px }
         a { display: block; height: 30px }
         b { display: block; height: 30px; background-color: #246 }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 200.0, 200.0);

    let target = second_child(&store);
    let ink = {
        let frag = store.fragments_of_box(target)[0];
        store
            .fragment(frag)
            .expect("live")
            .device_ink(&zgui_scene::Placements::of(&frame.spatial))
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
fn a_repaint_mark_on_a_paintless_box_damages_nothing() {
    // The other half of the contract above. A keyed list that splices its children marks every
    // retained row, and each row's container paints nothing — no background, no border, no
    // outline, no shadow — so absorbing its ink would stripe the whole port with damage on every
    // frame the window over it shifts. A style that starts painting flips the fragment's flags,
    // which the geometry compare reads, so that change is damaged as a change rather than here.
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
    let node = store.node(target).source;

    frame.damage = DamageSet::new();
    let root = store.root().expect("a root");
    let mut dirty = OnlyThis {
        node,
        bits: Dirty::REPAINT,
        marked: Vec::new(),
    };
    fragments(&mut frame, &mut store, root, &mut dirty);

    assert!(
        frame.damage.is_empty(),
        "a repaint of nothing damaged {:?}",
        frame.damage.rects()
    );
}

#[test]
fn a_repaint_mark_on_a_translucent_paintless_box_damages_its_subtree() {
    // An opacity applies to everything below the box, so a repaint of it changes every pixel
    // its subtree paints. The box paints nothing, and its child reaches below it, so neither its
    // own ink nor its border box holds those pixels.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("a"),
            Element::new("b").children(vec![Element::new("c")]),
        ]),
        "root { display: block; width: 200px }
         a { display: block; height: 30px }
         b { display: block; height: 10px; opacity: 0.5 }
         c { display: block; height: 30px; background-color: #246 }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 200.0, 200.0);

    let target = second_child(&store);
    let child = store.node(target).children[0];
    let ink = {
        let frag = store.fragments_of_box(child)[0];
        store
            .fragment(frag)
            .expect("live")
            .device_ink(&zgui_scene::Placements::of(&frame.spatial))
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

    assert!(
        covers(&frame.damage, ink),
        "the faded child is not in {:?}",
        frame.damage.rects()
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
        store
            .fragment(frag)
            .expect("live")
            .device_ink(&zgui_scene::Placements::of(&frame.spatial))
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
        store
            .fragment(frag)
            .expect("live")
            .device_ink(&zgui_scene::Placements::of(&frame.spatial))
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
    let vacated = store
        .fragment(last)
        .expect("live")
        .device_ink(&zgui_scene::Placements::of(&frame.spatial));

    // Laid out much wider, the paragraph fits on fewer lines and the last of them ceases to exist.
    frame.damage = DamageSet::new();
    {
        let mut tree = zgui_layout::tree::LayoutTree::new(
            &mut store,
            &mut content,
            zgui_layout::DeviceStyle::default(),
        );
        assert!(tree.layout_root(cephal::Size {
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

/// A dirty answer for one semantic element, recording what the pass reported about it.
struct Semantic {
    /// The element that owes work, and declares semantics.
    node: Option<zgui_dom::NodeKey>,
    /// What it owes.
    bits: Dirty,
    /// What was marked while the pass ran.
    marked: Vec<(Option<zgui_dom::NodeKey>, Dirty)>,
    /// The elements reported as moved.
    moved: Vec<Option<zgui_dom::NodeKey>>,
}

impl FrameDirty for Semantic {
    fn own(&self, node: Option<zgui_dom::NodeKey>) -> Dirty {
        if node.is_some() && node == self.node {
            self.bits
        } else {
            Dirty::empty()
        }
    }

    fn subtree(&self, _node: Option<zgui_dom::NodeKey>) -> Dirty {
        Dirty::all()
    }

    fn mark(&mut self, node: Option<zgui_dom::NodeKey>, bits: Dirty) {
        self.marked.push((node, bits));
    }

    fn moved(&mut self, node: Option<zgui_dom::NodeKey>) {
        self.moved.push(node);
    }

    fn is_semantic(&self, node: Option<zgui_dom::NodeKey>) -> bool {
        node.is_some() && node == self.node
    }

    fn retire(&mut self, _node: Option<zgui_dom::NodeKey>, _phase: Dirty) {}
}

/// A box that only grew is reported as moved — its rectangle and nothing else — while a box
/// that started clipping its children owes the whole projection.
#[test]
fn a_box_that_only_grew_owes_the_accessibility_tree_its_rectangle_alone() {
    let mut fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("a"), Element::new("b")]),
        "root { display: block; width: 200px }
         a { display: block; height: 30px }
         b { display: block; height: 30px; background-color: #246 }
         b.tall { height: 50px }
         b.clips { overflow: hidden }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 200.0, 200.0);
    let target = second_child(&store);
    let node = store.node(target).source;
    let index = fixture
        .document
        .store()
        .index_of(node.expect("the box came from an element"))
        .expect("a live element");

    fixture.edit_restyle_and_patch(&mut store, |edit| {
        edit.set_style_property(index, "height", Some("50px"));
    });
    support::lay_out_only(&mut store, &mut content, 200.0, 200.0);
    let root = store.root().expect("a root");
    let mut dirty = Semantic {
        node,
        bits: Dirty::RELAYOUT,
        marked: Vec::new(),
        moved: Vec::new(),
    };
    fragments(&mut frame, &mut store, root, &mut dirty);
    assert_eq!(
        dirty.moved,
        vec![node],
        "a box that grew was not reported as moved"
    );
    assert!(
        !dirty
            .marked
            .iter()
            .any(|(marked, bits)| *marked == node && bits.contains(Dirty::A11Y)),
        "a box that only grew was marked for a whole projection: {:?}",
        dirty.marked
    );

    fixture.edit_restyle_and_patch(&mut store, |edit| {
        edit.set_style_property(index, "overflow", Some("hidden"));
    });
    support::lay_out_only(&mut store, &mut content, 200.0, 200.0);
    let mut dirty = Semantic {
        node,
        bits: Dirty::RELAYOUT | Dirty::REFRAGMENT,
        marked: Vec::new(),
        moved: Vec::new(),
    };
    fragments(&mut frame, &mut store, root, &mut dirty);
    assert!(
        dirty
            .marked
            .iter()
            .any(|(marked, bits)| *marked == node && bits.contains(Dirty::A11Y)),
        "a box that started clipping its children was answered with a rectangle: {:?}",
        dirty.marked
    );
}

/// A paragraph of `aa ` and a span holding `bbbb`, with the span's element.
fn spanned(span: &str) -> (LayoutStore, Fixture, support::Frame, BoxKey) {
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("para")
                .text("aa ")
                .children(vec![Element::new("span").text("bbbb")]),
        ]),
        &format!(
            "root {{ display: block; width: 200px }}
             para {{ display: block }}
             span {{ display: inline; {span} }}"
        ),
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let frame = lay_out(&mut store, &mut content, 200.0, 200.0);
    // The box holding the lines, which may be an anonymous one below the paragraph.
    let mut stack = vec![store.root().expect("a root")];
    let mut lines = None;
    while let Some(key) = stack.pop() {
        if store.inline_resolution(key).is_some() {
            lines = Some(key);
            break;
        }
        stack.extend(store.node(key).children.iter().copied());
    }
    let lines = lines.expect("a context");
    (store, fixture, frame, lines)
}

/// The nested inline box inside a context.
fn span_of(store: &LayoutStore, context: BoxKey) -> BoxKey {
    *store
        .node(context)
        .children
        .iter()
        .find(|child| store.node(**child).text.is_none())
        .expect("a span")
}

#[test]
fn a_repaint_of_a_painted_span_damages_the_line_that_paints_it() {
    // The span's background is painted by its paragraph's line, and the mark is on the span,
    // below the box that holds the line.
    let (mut store, _fixture, mut frame, para) = spanned("background-color: #246");
    let span = span_of(&store, para);
    let line = store.fragments_of_box(para)[1];
    let ink = store
        .fragment(line)
        .expect("live")
        .device_ink(&zgui_scene::Placements::of(&frame.spatial));

    frame.damage = DamageSet::new();
    let root = store.root().expect("a root");
    let mut dirty = OnlyThis {
        node: store.node(span).source,
        bits: Dirty::REPAINT,
        marked: Vec::new(),
    };
    fragments(&mut frame, &mut store, root, &mut dirty);
    assert!(covers(&frame.damage, ink), "the line was not repainted");
}

#[test]
fn a_repaint_damages_its_ink_where_the_document_space_draws_it() {
    // Ink is measured in the fragment's anchor and damage on the device. Here the document's own
    // space is moved 300px to the right, as the poison test mode moves it to the left, so a pass
    // that absorbed the raw ink would damage pixels 300px away from the repainted box.
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("a"), Element::new("b")]),
        "root { display: block; width: 200px }
         a { display: block; height: 30px }
         b { display: block; height: 30px; background-color: #246 }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 200.0, 200.0);

    let viewport = frame.spatial.viewport();
    let held = frame
        .spatial
        .resolve(frame.spatial.document())
        .expect("the document space resolves");
    let document = frame.spatial.establish(
        zgui_scene::PropertyOwner::DOCUMENT,
        zgui_scene::SpatialNode {
            parent: Some(viewport),
            local: held.then(&zgui_geom::Matrix4::translation(300.0, 0.0, 0.0)),
            anchoring: zgui_scene::Anchoring::Scrolling,
        },
    );
    let root = store.root().expect("a root");
    fragments(&mut frame, &mut store, root, &mut Everything);

    let target = second_child(&store);
    let placements = zgui_scene::Placements::of(&frame.spatial);
    let (raw, device) = {
        let fragment = store
            .fragment(store.fragments_of_box(target)[0])
            .expect("live");
        assert_eq!(
            fragment.anchor, document,
            "the ink is measured in the document space"
        );
        (fragment.ink, fragment.device_ink(&placements))
    };
    assert_eq!(device.origin.x.0, raw.origin.x.0 + 300.0 + held.get(0, 3));

    frame.damage = DamageSet::new();
    let mut dirty = OnlyThis {
        node: store.node(target).source,
        bits: Dirty::REPAINT,
        marked: Vec::new(),
    };
    fragments(&mut frame, &mut store, root, &mut dirty);

    assert!(
        covers(&frame.damage, device),
        "the repainted piece is not damaged where it is drawn: {:?}",
        frame.damage.rects()
    );
    assert!(
        !covers(&frame.damage, raw),
        "the raw ink was damaged as device pixels: {:?}",
        frame.damage.rects()
    );
}

#[test]
fn a_resized_box_damages_its_old_ink_where_the_last_publish_drew_it() {
    // Old ink goes on the device through the matrices of the last publish, and new ink through the
    // tree the pass writes. Here the document's space moves 300px back to the left between the two,
    // so a pass that put the old ink through the current tree would leave the old pixels standing.
    let mut fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("a"), Element::new("b")]),
        "root { display: block; width: 200px }
         a { display: block; height: 30px }
         b { display: block; height: 30px; background-color: #246 }
         b.tall { height: 50px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 200.0, 200.0);

    let viewport = frame.spatial.viewport();
    let held = frame
        .spatial
        .resolve(frame.spatial.document())
        .expect("the document space resolves");
    let document_at = |offset: f32| zgui_scene::SpatialNode {
        parent: Some(viewport),
        local: held.then(&zgui_geom::Matrix4::translation(offset, 0.0, 0.0)),
        anchoring: zgui_scene::Anchoring::Scrolling,
    };
    frame
        .spatial
        .establish(zgui_scene::PropertyOwner::DOCUMENT, document_at(300.0));
    let root = store.root().expect("a root");
    fragments(&mut frame, &mut store, root, &mut Everything);

    let target = second_child(&store);
    let ink_of = |store: &LayoutStore, placements: &zgui_scene::Placements| {
        store
            .fragment(store.fragments_of_box(target)[0])
            .expect("live")
            .device_ink(placements)
    };
    let drawn = zgui_scene::Placements::of(&frame.spatial);
    let old = ink_of(&store, &drawn);

    frame
        .spatial
        .establish(zgui_scene::PropertyOwner::DOCUMENT, document_at(0.0));
    let node = store.node(target).source;
    let index = fixture
        .document
        .store()
        .index_of(node.expect("the box came from an element"))
        .expect("a live element");
    fixture.edit_restyle_and_patch(&mut store, |edit| {
        edit.add_class(index, zgui_interned::ClassName::new("tall"));
    });
    support::lay_out_only(&mut store, &mut content, 200.0, 200.0);
    frame.damage = DamageSet::new();
    let mut dirty = OnlyThis {
        node,
        bits: Dirty::RELAYOUT,
        marked: Vec::new(),
    };
    support::fragments_after(&mut frame, &mut store, root, &mut dirty, &drawn);
    let new = ink_of(&store, &zgui_scene::Placements::of(&frame.spatial));

    assert_eq!(old.origin.x.0 - new.origin.x.0, 300.0);
    assert!(
        covers(&frame.damage, old),
        "the old ink is not damaged where the last publish drew it: {old:?} in {:?}",
        frame.damage.rects()
    );
    assert!(
        covers(&frame.damage, new),
        "the new ink is not damaged where it is drawn: {new:?} in {:?}",
        frame.damage.rects()
    );
}
