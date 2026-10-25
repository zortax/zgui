//! A scroll container that draws its content in a space of its own, against one that offsets it.
//!
//! With [`scroll_nodes`] on, a scroll moves one coordinate system and leaves every piece below the
//! container as it is. The pixels are only the same if every piece is drawn where composing it
//! would have put it, so the device rectangles of both documents are compared after each scroll.
//! The other claim is the cost: a scroll after the first visits the path down to the container and
//! touches no entry of the hit index below it.
//!
//! Its own test target, because the switch and the counters are process-wide.

mod support;

use std::sync::{Mutex, MutexGuard, PoisonError};

use support::{Element, Fixture, Frame, lay_out, measurer};
use zgui_bits::Dirty;
use zgui_geom::{Device, DevicePx, Point, Rect};
use zgui_layout::fragment::diff::{FrameDirty, RigidMoves};
use zgui_layout::fragment::scroll_nodes;
use zgui_layout::fragment::transform::placed::onto_device;
use zgui_layout::{FragmentFlags, LayoutStore};
use zgui_profile::{COUNTERS_ENABLED, Counter, counter};
use zgui_scene::Placements;

/// The switch and the counter block are process-wide, so every case runs alone.
fn exclusive() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A scrollport holding `rows` rows, each a rounded card that clips the text inside it.
///
/// The cards' clips are minted in the content space, and the text and its lines are pieces of more
/// than one kind, so a piece drawn in the wrong space shows up in more than one comparison.
fn page(rows: usize) -> Fixture {
    let rows: Vec<Element> = (0..rows)
        .map(|_| {
            Element::new("row").children(vec![Element::new("card").text("alpha bravo delta gamma")])
        })
        .collect();
    Fixture::new(
        Element::new("root").children(vec![Element::new("port").children(rows)]),
        "root { display: block; width: 300px; height: 240px }
         port { display: block; height: 200px; overflow: scroll; margin-top: 20px }
         row  { display: block; height: 20px }
         card { display: block; overflow: hidden; border-radius: 4px; height: 18px }",
    )
}

/// The scrollport's box.
fn port_box(store: &LayoutStore) -> zgui_layout::BoxKey {
    let root = store.root().expect("a root box");
    store.node(root).children[0]
}

/// The element the scrollport was generated from.
fn port(store: &LayoutStore) -> zgui_dom::NodeKey {
    store
        .get(port_box(store))
        .and_then(|node| node.source)
        .expect("the scrollport came from an element")
}

/// One element owes a scroll, and nothing else owes anything.
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
        self.own(node)
    }

    fn mark(&mut self, _node: Option<zgui_dom::NodeKey>, _bits: Dirty) {}

    fn retire(&mut self, _node: Option<zgui_dom::NodeKey>, _phase: Dirty) {}
}

/// Composes the fragments under `dirty`, and answers the walk's moves.
fn compose(frame: &mut Frame, store: &mut LayoutStore, dirty: &mut impl FrameDirty) -> RigidMoves {
    let root = store.root().expect("a root box");
    let drawn = Placements::of(&frame.spatial);
    let mut tables = zgui_layout::fragment::build::Tables {
        clips: &mut frame.clips,
        spatial: &mut frame.spatial,
        drawn: &drawn,
        device: zgui_layout::style::DeviceStyle::default(),
        scroll: &frame.scroll,
        placements: &[],
    };
    let moves = zgui_layout::fragment::diff::rebuild(
        store,
        &mut frame.hit,
        &mut tables,
        dirty,
        root,
        &mut frame.damage,
    );
    let violations = zgui_layout::invariants::check(store, &frame.hit);
    assert!(violations.is_empty(), "{violations:?}");
    moves
}

/// A rectangle as four numbers that compare.
fn numbers(rect: Rect<DevicePx, Device>) -> [f32; 4] {
    [
        rect.origin.x.0,
        rect.origin.y.0,
        rect.size.width.0,
        rect.size.height.0,
    ]
}

/// One fragment as it is drawn: its kind, its border box and its ink on the device, and the
/// device rectangle its clip chain admits.
type Drawn = (String, [f32; 4], [f32; 4], [f32; 4]);

/// Every fragment in painting order, as it is drawn on the device.
fn drawn(frame: &Frame, store: &LayoutStore) -> Vec<Drawn> {
    let placements = Placements::of(&frame.spatial);
    let root = store.root().expect("a root box");
    let mut out = Vec::new();
    for box_ in zgui_layout::fragment::stacking::paint_order(store, root) {
        for frag in store.fragments_of_box(box_) {
            let fragment = store.fragment(*frag).expect("a live fragment");
            let clip = frame
                .clips
                .bounds_placed(fragment.clip, &|id| frame.spatial.resolve(id));
            out.push((
                format!("{:?}", fragment.kind),
                numbers(onto_device(
                    fragment.border_box,
                    fragment.transform,
                    &placements,
                )),
                numbers(fragment.device_ink(&placements)),
                numbers(clip),
            ));
        }
    }
    out
}

/// The same scrolls over two documents, one offsetting its content and one drawing it in a
/// content space, and what each draws after every scroll.
fn both(offsets: &[f32]) -> (Vec<Vec<Drawn>>, Vec<Vec<Drawn>>) {
    let mut answers = Vec::new();
    for nodes in [false, true] {
        scroll_nodes::set_enabled(nodes);
        let fixture = page(12);
        let mut store = fixture.box_tree();
        let mut content = measurer();
        let mut frame = lay_out(&mut store, &mut content, 300.0, 240.0);
        let element = port(&store);
        let mut after = Vec::new();
        for &by in offsets {
            frame
                .scroll
                .place(element, Point::new(DevicePx(0.0), DevicePx(by)));
            compose(&mut frame, &mut store, &mut Scrolled { node: element });
            after.push(drawn(&frame, &store));
        }
        answers.push(after);
    }
    scroll_nodes::set_enabled(false);
    let spaced = answers.pop().expect("both documents ran");
    let offset = answers.pop().expect("both documents ran");
    (offset, spaced)
}

#[test]
fn a_content_space_draws_every_piece_where_offsetting_it_would() {
    let _guard = exclusive();
    let (offset, spaced) = both(&[37.0, 12.5, 57.0, 0.0]);
    for (step, (offset, spaced)) in offset.iter().zip(&spaced).enumerate() {
        assert!(
            offset.len() > 30,
            "the fixture produced {} fragments, too few for the comparison to mean anything",
            offset.len()
        );
        assert_eq!(
            offset, spaced,
            "after scroll {step}, the content space draws a piece somewhere else"
        );
    }
}

#[test]
fn a_scroll_writes_the_content_space_and_leaves_the_rectangles_alone() {
    let _guard = exclusive();
    scroll_nodes::set_enabled(true);
    let fixture = page(12);
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 300.0, 240.0);
    let element = port(&store);
    let port_box = port_box(&store);

    frame
        .scroll
        .place(element, Point::new(DevicePx(0.0), DevicePx(10.0)));
    compose(&mut frame, &mut store, &mut Scrolled { node: element });
    let piece = store.fragments_of_box(port_box)[0];
    assert!(
        store
            .fragment(piece)
            .is_some_and(|piece| piece.flags.contains(FragmentFlags::SCROLLS_CONTENT)),
        "the first scroll did not give the port a content space"
    );
    let row = store.node(port_box).children[3];
    let first = store.fragments_of_box(row)[0];
    let held = store.fragment(first).expect("a row piece").border_box;
    let was = onto_device(
        held,
        store.fragment(first).expect("a row piece").transform,
        &Placements::of(&frame.spatial),
    );

    frame
        .scroll
        .place(element, Point::new(DevicePx(0.0), DevicePx(30.0)));
    frame.damage = zgui_bits::DamageSet::new();
    let moves = compose(&mut frame, &mut store, &mut Scrolled { node: element });
    scroll_nodes::set_enabled(false);
    // The scroll moved every pixel of the port, and the walk visited no row to damage it. So the
    // port itself is the damage, and it holds every row the port shows, before and after.
    let port_piece = store
        .fragment(store.fragments_of_box(port_box)[0])
        .expect("a port piece");
    let port_on_device = onto_device(
        port_piece.padding_box,
        port_piece.transform,
        &Placements::of(&frame.spatial),
    );
    let pixels = zgui_layout::fragment::diff::pixels(port_on_device);
    assert!(
        frame
            .damage
            .rects()
            .iter()
            .any(|held| held.contains_rect(pixels)),
        "the scroll damaged {:?}, which leaves part of the port {pixels:?} out",
        frame.damage.rects()
    );
    let fragment = store.fragment(first).expect("a row piece");
    assert_eq!(
        fragment.border_box, held,
        "the row's rectangle moved, so the scroll offset it rather than writing the space"
    );
    assert_eq!(
        moves.by,
        Some((0.0, -20.0)),
        "the move is the change of offset"
    );
    assert_eq!(moves.count, 1, "one move for the whole container");
    let placements = Placements::of(&frame.spatial);
    let device = onto_device(fragment.border_box, fragment.transform, &placements);
    assert_eq!(
        device.origin.y.0,
        was.origin.y.0 - 20.0,
        "the space did not carry the row by the change of offset"
    );
}

/// What one scroll of the port costs after the first, over `rows` rows: the boxes visited, the hit
/// entries updated and the walk's moves.
fn second_scroll(rows: usize, nodes: bool) -> (u64, u64, u64, RigidMoves) {
    scroll_nodes::set_enabled(nodes);
    let fixture = page(rows);
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 300.0, 240.0);
    let element = port(&store);
    let bars = store.fragments_of_box(port_box(&store)).len() as u64 - 1;
    frame
        .scroll
        .place(element, Point::new(DevicePx(0.0), DevicePx(40.0)));
    compose(&mut frame, &mut store, &mut Scrolled { node: element });
    frame
        .scroll
        .place(element, Point::new(DevicePx(0.0), DevicePx(60.0)));
    counter::reset();
    let moves = compose(&mut frame, &mut store, &mut Scrolled { node: element });
    let visited = counter::get(Counter::NodesVisited);
    let entries = counter::get(Counter::HitEntriesUpdated);
    scroll_nodes::set_enabled(false);
    (visited, entries, bars, moves)
}

#[test]
fn a_scroll_after_the_first_visits_the_path_down_and_no_row() {
    let _guard = exclusive();
    // The control: the same scroll without a content space visits and carries every row.
    let (visited_off, entries_off, _, moves_off) = second_scroll(1000, false);
    let (visited, entries, bars, moves) = second_scroll(1000, true);
    assert_eq!(moves_off.by, Some((0.0, -20.0)));
    assert_eq!(moves.by, Some((0.0, -20.0)));
    if !COUNTERS_ENABLED {
        return;
    }
    assert!(
        visited_off > 1000,
        "the control visited {visited_off} boxes, so it did not carry the rows"
    );
    assert!(
        entries_off > 1000,
        "the control updated {entries_off} hit entries, so it did not carry the rows"
    );
    assert!(
        visited <= 2,
        "a scroll visited {visited} boxes; the path down to the port is two"
    );
    assert!(
        entries <= bars,
        "a scroll updated {entries} hit entries; only the port's {bars} bar pieces may move"
    );
}

/// One element owes `bits`, and the elements above it owe the same below them. A scrolled
/// element, when there is one, owes a scroll in the same frame.
struct Owing {
    /// The element that changed.
    target: zgui_dom::NodeKey,
    /// The elements above it.
    above: Vec<zgui_dom::NodeKey>,
    /// What it owes.
    bits: Dirty,
    /// The element that scrolls in the same frame, which is one of `above`.
    scrolled: Option<zgui_dom::NodeKey>,
}

impl FrameDirty for Owing {
    fn own(&self, node: Option<zgui_dom::NodeKey>) -> Dirty {
        let mut bits = Dirty::empty();
        if node == Some(self.target) {
            bits |= self.bits;
        }
        if node.is_some() && node == self.scrolled {
            bits |= Dirty::SCROLL;
        }
        bits
    }

    fn subtree(&self, node: Option<zgui_dom::NodeKey>) -> Dirty {
        if node == Some(self.target) || node.is_some_and(|node| self.above.contains(&node)) {
            self.bits | self.own(node)
        } else {
            Dirty::empty()
        }
    }

    fn mark(&mut self, _node: Option<zgui_dom::NodeKey>, _bits: Dirty) {}

    fn retire(&mut self, _node: Option<zgui_dom::NodeKey>, _phase: Dirty) {}
}

/// What a row that turns fixed draws in both documents, with the port scrolled in the same frame
/// when `scroll_too` holds.
fn fixed_row(scroll_too: bool) {
    let mut answers = Vec::new();
    for nodes in [false, true] {
        scroll_nodes::set_enabled(nodes);
        let mut fixture = page(12);
        let mut store = fixture.box_tree();
        let mut content = measurer();
        let mut frame = lay_out(&mut store, &mut content, 300.0, 240.0);
        let element = port(&store);
        for by in [30.0, 40.0] {
            frame
                .scroll
                .place(element, Point::new(DevicePx(0.0), DevicePx(by)));
            compose(&mut frame, &mut store, &mut Scrolled { node: element });
        }
        let port_box = port_box(&store);
        let row = store.node(port_box).children[3];
        let target = store
            .node(row)
            .source
            .expect("the row came from an element");
        let index = fixture
            .document
            .store()
            .index_of(target)
            .expect("a live element");
        fixture.edit_restyle_and_patch(&mut store, |edit| {
            edit.set_style_property(index, "position", Some("fixed"));
            edit.set_style_property(index, "top", Some("0px"));
            edit.set_style_property(index, "width", Some("100px"));
        });
        support::lay_out_only(&mut store, &mut content, 300.0, 240.0);
        let root = store.root().expect("a root box");
        let above = vec![
            store
                .node(root)
                .source
                .expect("the root came from an element"),
            element,
        ];
        if scroll_too {
            frame
                .scroll
                .place(element, Point::new(DevicePx(0.0), DevicePx(55.0)));
        }
        let mut dirty = Owing {
            target,
            above,
            bits: Dirty::RELAYOUT | Dirty::REPAINT,
            scrolled: scroll_too.then_some(element),
        };
        compose(&mut frame, &mut store, &mut dirty);
        let held = store
            .fragment(store.fragments_of_box(port_box)[0])
            .is_some_and(|piece| piece.flags.contains(FragmentFlags::SCROLLS_CONTENT));
        assert!(!held, "the port kept its content space over a fixed row");
        answers.push(drawn(&frame, &store));
    }
    scroll_nodes::set_enabled(false);
    let spaced = answers.pop().expect("both documents ran");
    let offset = answers.pop().expect("both documents ran");
    assert_eq!(
        offset, spaced,
        "the fixed row is drawn somewhere else in the document that had a content space"
    );
}

#[test]
fn a_row_that_turns_fixed_inside_a_content_space_is_drawn_where_composing_it_would() {
    let _guard = exclusive();
    fixed_row(false);
}

#[test]
fn a_row_that_turns_fixed_as_its_port_scrolls_is_drawn_where_composing_it_would() {
    let _guard = exclusive();
    fixed_row(true);
}

/// The same scrollport inside a fixed panel, which has a coordinate system of its own and no
/// matrix.
fn page_in_panel(rows: usize) -> Fixture {
    let rows: Vec<Element> = (0..rows)
        .map(|_| {
            Element::new("row").children(vec![Element::new("card").text("alpha bravo delta gamma")])
        })
        .collect();
    Fixture::new(
        Element::new("root").children(vec![
            Element::new("panel").children(vec![Element::new("port").children(rows)]),
        ]),
        "root  { display: block; width: 300px; height: 240px }
         panel { display: block; position: fixed; top: 0px; left: 0px; width: 300px; height: 240px }
         port  { display: block; height: 200px; overflow: scroll; margin-top: 20px }
         row   { display: block; height: 20px }
         card  { display: block; overflow: hidden; border-radius: 4px; height: 18px }",
    )
}

#[test]
fn a_content_space_under_a_panel_that_takes_a_transform_is_drawn_and_damaged_where_composing_it_would()
 {
    let _guard = exclusive();
    let mut answers = Vec::new();
    for nodes in [false, true] {
        scroll_nodes::set_enabled(nodes);
        let mut fixture = page_in_panel(12);
        let mut store = fixture.box_tree();
        let mut content = measurer();
        let mut frame = lay_out(&mut store, &mut content, 300.0, 240.0);
        let root = store.root().expect("a root box");
        let panel_box = store.node(root).children[0];
        let port_box = store.node(panel_box).children[0];
        let element = store
            .node(port_box)
            .source
            .expect("the scrollport came from an element");
        let source = |key| {
            store
                .node(key)
                .source
                .expect("the box came from an element")
        };
        let (root_element, panel) = (source(root), source(panel_box));
        for by in [30.0, 40.0] {
            frame
                .scroll
                .place(element, Point::new(DevicePx(0.0), DevicePx(by)));
            let mut scrolled = Owing {
                target: element,
                above: vec![root_element, panel],
                bits: Dirty::SCROLL,
                scrolled: None,
            };
            compose(&mut frame, &mut store, &mut scrolled);
        }
        let held = store
            .fragment(store.fragments_of_box(port_box)[0])
            .is_some_and(|piece| piece.flags.contains(FragmentFlags::SCROLLS_CONTENT));
        assert_eq!(
            held, nodes,
            "the port took a content space only with the switch on"
        );
        // The panel takes a matrix and nothing below it changes, so the port's subtree is carried
        // onto the new matrix and not composed again.
        let target = panel;
        let index = fixture
            .document
            .store()
            .index_of(target)
            .expect("a live element");
        fixture.edit_restyle_and_patch(&mut store, |edit| {
            edit.set_style_property(index, "transform", Some("translate(8px, 6px) scale(2)"));
        });
        let above = vec![root_element];
        frame.damage = zgui_bits::DamageSet::new();
        let before = drawn(&frame, &store);
        compose(
            &mut frame,
            &mut store,
            &mut Owing {
                target,
                above,
                bits: Dirty::REPAINT,
                scrolled: None,
            },
        );
        let after = drawn(&frame, &store);
        assert_ne!(before, after, "the panel's matrix moved nothing");
        answers.push((after, frame.damage.bounds()));
    }
    scroll_nodes::set_enabled(false);
    let (spaced, spaced_damage) = answers.pop().expect("both documents ran");
    let (offset, offset_damage) = answers.pop().expect("both documents ran");
    assert_eq!(
        offset, spaced,
        "under the panel's new matrix, the content space draws a piece somewhere else"
    );
    assert_eq!(
        offset_damage, spaced_damage,
        "under the panel's new matrix, the content space damages somewhere else"
    );
}
