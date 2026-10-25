//! A scroll container that draws its content in a space of its own, against a fresh build.
//!
//! A scroll moves one coordinate system and leaves every piece below the container as it is (see
//! [`scroll_nodes`](zgui_layout::fragment::scroll_nodes)). The pixels are only the same if every
//! piece is drawn where composing it would have put it, so the device rectangles of the scrolled
//! document are compared with a document composed fresh at the same offset. The other claim is the
//! cost: a scroll after the first visits the path down to the container and touches no entry of
//! the hit index below it.
//!
//! Its own test target, because the counters are process-wide.

mod support;

use std::sync::{Mutex, MutexGuard, PoisonError};

use support::{Element, Fixture, Frame, lay_out, measurer};
use zgui_bits::Dirty;
use zgui_geom::{Device, DevicePx, Point, Rect};
use zgui_layout::fragment::diff::{FrameDirty, RigidMoves};
use zgui_layout::fragment::transform::placed::onto_device;
use zgui_layout::{FragmentFlags, LayoutStore};
use zgui_profile::{COUNTERS_ENABLED, Counter, counter};
use zgui_scene::Placements;

/// The counter block is process-wide, so every case runs alone.
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
    drawn_except(frame, store, &[])
}

/// The same, without the fragments of the boxes in `skip`.
fn drawn_except(frame: &Frame, store: &LayoutStore, skip: &[zgui_layout::BoxKey]) -> Vec<Drawn> {
    let placements = Placements::of(&frame.spatial);
    let root = store.root().expect("a root box");
    let mut out = Vec::new();
    for box_ in zgui_layout::fragment::stacking::paint_order(store, root) {
        if skip.contains(&box_) {
            continue;
        }
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

#[test]
fn a_content_space_draws_every_piece_where_a_fresh_build_puts_it() {
    let _guard = exclusive();
    let (spaced, fresh, held) = two_ways(&|| page(12), &[37.0, 12.5, 57.0, 0.0]);
    assert!(
        held.iter().all(|held| *held),
        "the port gave its content space up: {held:?}"
    );
    for (step, spaced) in spaced.iter().enumerate() {
        assert!(
            spaced.len() > 30,
            "after scroll {step}, the fixture produced {} fragments, too few for the comparison \
             to mean anything",
            spaced.len()
        );
    }
    agree(&spaced, &fresh);
}

#[test]
fn a_scroll_writes_the_content_space_and_leaves_the_rectangles_alone() {
    let _guard = exclusive();
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
fn second_scroll(rows: usize) -> (u64, u64, u64, RigidMoves) {
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
    (visited, entries, bars, moves)
}

#[test]
fn a_scroll_after_the_first_visits_the_path_down_and_no_row() {
    let _guard = exclusive();
    let (visited, entries, bars, moves) = second_scroll(1000);
    assert_eq!(moves.by, Some((0.0, -20.0)));
    assert_eq!(moves.count, 1, "one move for the whole container");
    if !COUNTERS_ENABLED {
        return;
    }
    assert!(
        visited <= 2,
        "a scroll visited {visited} boxes; the path down to the port is two"
    );
    assert!(
        entries <= bars,
        "a scroll updated {entries} hit entries; only the port's {bars} bar pieces may move"
    );
}

#[test]
fn a_new_port_takes_its_content_space_and_its_first_scroll_visits_no_row() {
    let _guard = exclusive();
    let fixture = page(1000);
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 300.0, 240.0);
    assert!(
        scrolls_content(&store, port_box(&store)),
        "the new port took no content space at its first composition"
    );
    let element = port(&store);
    let bars = store.fragments_of_box(port_box(&store)).len() as u64 - 1;
    frame
        .scroll
        .place(element, Point::new(DevicePx(0.0), DevicePx(40.0)));
    counter::reset();
    let moves = compose(&mut frame, &mut store, &mut Scrolled { node: element });
    let visited = counter::get(Counter::NodesVisited);
    let entries = counter::get(Counter::HitEntriesUpdated);
    assert_eq!(moves.by, Some((0.0, -40.0)), "the move is the first offset");
    assert_eq!(moves.count, 1, "one move for the whole container");
    if !COUNTERS_ENABLED {
        return;
    }
    assert!(
        visited <= 2,
        "the first scroll visited {visited} boxes; the path down to the port is two"
    );
    assert!(
        entries <= bars,
        "the first scroll updated {entries} hit entries; only the port's {bars} bar pieces may move"
    );
}

#[test]
fn a_new_port_composed_at_an_offset_draws_every_piece_where_a_scrolled_one_does() {
    let _guard = exclusive();
    let (scrolled, _, held) = two_ways(&|| page(12), &[0.0, 37.0]);
    assert!(held[1], "the scrolled port gave its content space up");
    let fixture = page(12);
    let element = port(&fixture.box_tree());
    let mut store = fixture.box_tree();
    let mut content = measurer();
    support::lay_out_only(&mut store, &mut content, 300.0, 240.0);
    let mut frame = Frame::new();
    frame
        .scroll
        .place(element, Point::new(DevicePx(0.0), DevicePx(37.0)));
    compose(
        &mut frame,
        &mut store,
        &mut zgui_layout::fragment::diff::Everything,
    );
    assert!(
        scrolls_content(&store, port_box(&store)),
        "a port composed first at an offset took no content space"
    );
    assert_eq!(
        drawn(&frame, &store),
        scrolled[1],
        "a port composed first at an offset draws a piece somewhere the scrolled port does not"
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

/// What a row that turns fixed draws, against composing every box again, with the port scrolled
/// in the same frame when `scroll_too` holds.
fn fixed_row(scroll_too: bool) {
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
    let last = if scroll_too { 55.0 } else { 40.0 };
    frame
        .scroll
        .place(element, Point::new(DevicePx(0.0), DevicePx(last)));
    let mut dirty = Owing {
        target,
        above,
        bits: Dirty::RELAYOUT | Dirty::REPAINT,
        scrolled: scroll_too.then_some(element),
    };
    compose(&mut frame, &mut store, &mut dirty);
    assert!(
        scrolls_content(&store, port_box),
        "a fixed row hangs outside the content space, so the port keeps it"
    );
    // The oracle composes every box again over the same box tree. A fresh build would build
    // another tree, because the restyle above keeps the fixed row where the port holds it.
    let scrolled = drawn(&frame, &store);
    compose(
        &mut frame,
        &mut store,
        &mut zgui_layout::fragment::diff::Everything,
    );
    assert_eq!(
        scrolled,
        drawn(&frame, &store),
        "the fixed row is drawn somewhere composing every box again does not draw it"
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
    assert!(
        scrolls_content(&store, port_box),
        "the port took no content space"
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
    let panel_ink = |store: &LayoutStore, frame: &Frame| {
        store
            .fragment(store.fragments_of_box(panel_box)[0])
            .expect("a panel piece")
            .device_subtree_ink(&Placements::of(&frame.spatial))
    };
    let was = panel_ink(&store, &frame);
    frame.damage = zgui_bits::DamageSet::new();
    let before = drawn(&frame, &store);
    compose(
        &mut frame,
        &mut store,
        &mut Owing {
            target,
            above: vec![root_element],
            bits: Dirty::REPAINT,
            scrolled: None,
        },
    );
    let after = drawn(&frame, &store);
    assert_ne!(before, after, "the panel's matrix moved nothing");
    assert_eq!(
        after,
        fresh(&fixture, element, 40.0),
        "under the panel's new matrix, the content space draws a piece somewhere a fresh build \
         does not"
    );
    // The damage holds the panel where it was and where it is, and nothing else: ink carried
    // through the matrix twice lands outside both.
    let is = panel_ink(&store, &frame);
    let damage = frame.damage.bounds().expect("the matrix damaged something");
    for rect in [was, is] {
        let pixels = zgui_layout::fragment::diff::pixels(rect);
        assert!(
            damage.contains_rect(pixels),
            "the damage {damage:?} leaves the panel's ink {pixels:?} out"
        );
    }
    let both = zgui_layout::fragment::diff::pixels(was.union(is));
    assert!(
        both.contains_rect(damage),
        "the damage {damage:?} reaches outside the panel's ink before and after, {both:?}"
    );
}

/// What a document composed fresh at scroll offset `by` draws: built from `fixture`'s document as
/// it stands, laid out, and composed once with `element` scrolled to `by` and no scroll before it.
fn fresh(fixture: &Fixture, element: zgui_dom::NodeKey, by: f32) -> Vec<Drawn> {
    let mut store = fixture.box_tree();
    let mut content = measurer();
    support::lay_out_only(&mut store, &mut content, 300.0, 240.0);
    let mut frame = Frame::new();
    frame
        .scroll
        .place(element, Point::new(DevicePx(0.0), DevicePx(by)));
    compose(
        &mut frame,
        &mut store,
        &mut zgui_layout::fragment::diff::Everything,
    );
    drawn(&frame, &store)
}

/// What each of the two documents of [`two_ways`] draws after every scroll, and whether the
/// scrolled port held a content space.
type TwoWays = (Vec<Vec<Drawn>>, Vec<Vec<Drawn>>, Vec<bool>);

/// The same scrolls over a document built from `make`, two ways: scrolled one offset after the
/// other, and composed fresh at each offset without a scroll before it.
///
/// Answers what each draws after every scroll, and whether the port of the scrolled document held
/// a content space after every scroll.
fn two_ways(make: &dyn Fn() -> Fixture, offsets: &[f32]) -> TwoWays {
    let fixture = make();
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 300.0, 240.0);
    let element = port(&store);
    let mut scrolled = Vec::new();
    let mut held = Vec::new();
    for &by in offsets {
        frame
            .scroll
            .place(element, Point::new(DevicePx(0.0), DevicePx(by)));
        compose(&mut frame, &mut store, &mut Scrolled { node: element });
        scrolled.push(drawn(&frame, &store));
        held.push(scrolls_content(&store, port_box(&store)));
    }
    let fresh = offsets
        .iter()
        .map(|&by| fresh(&make(), element, by))
        .collect();
    (scrolled, fresh, held)
}

/// Whether `key` draws its children in a content space.
fn scrolls_content(store: &LayoutStore, key: zgui_layout::BoxKey) -> bool {
    store
        .fragment(store.fragments_of_box(key)[0])
        .is_some_and(|piece| piece.flags.contains(FragmentFlags::SCROLLS_CONTENT))
}

/// Asserts the two answers of [`two_ways`] agree after every scroll.
fn agree(scrolled: &[Vec<Drawn>], fresh: &[Vec<Drawn>]) {
    assert_eq!(scrolled.len(), fresh.len());
    for (step, (scrolled, fresh)) in scrolled.iter().zip(fresh).enumerate() {
        assert_eq!(
            scrolled, fresh,
            "after scroll {step}, the scrolled document draws a piece somewhere a fresh build \
             does not"
        );
    }
}

/// A scrollport with a sticky header of its own, then sections that each hold a sticky header
/// and rows of clipped cards.
fn sticky_page() -> Fixture {
    let section = || {
        let mut children = vec![Element::new("head").text("alpha bravo")];
        for _ in 0..5 {
            children.push(
                Element::new("row")
                    .children(vec![Element::new("card").text("alpha bravo delta gamma")]),
            );
        }
        Element::new("section").children(children)
    };
    let mut children = vec![Element::new("top").text("delta gamma")];
    children.extend((0..6).map(|_| section()));
    Fixture::new(
        Element::new("root").children(vec![Element::new("port").children(children)]),
        "root    { display: block; width: 300px; height: 240px }
         port    { display: block; height: 200px; overflow: scroll; margin-top: 20px }
         top     { display: block; height: 16px; position: sticky; top: 0px }
         section { display: block }
         head    { display: block; height: 16px; position: sticky; top: 16px }
         row     { display: block; height: 20px }
         card    { display: block; overflow: hidden; border-radius: 4px; height: 18px }",
    )
}

/// The offsets the sticky and fixed pages are scrolled through: past the first header, back,
/// by a fraction, past several sections' ends and back to the top.
const STEPS: [f32; 8] = [10.0, 37.0, 120.0, 12.5, 250.0, 251.0, 400.0, 0.0];

#[test]
fn a_sticky_header_in_a_content_space_sticks_at_the_port_edge_where_a_fresh_build_puts_it() {
    let _guard = exclusive();
    let (spaced, fresh, held) = two_ways(&sticky_page, &STEPS);
    assert!(
        held.iter().all(|held| *held),
        "the port gave its content space up over sticky headers: {held:?}"
    );
    agree(&spaced, &fresh);

    // The port's own header sticks at the top edge of the port whenever the port is scrolled.
    let fixture = sticky_page();
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 300.0, 240.0);
    let element = port(&store);
    let port_key = port_box(&store);
    let top = store.node(port_key).children[0];
    for by in STEPS {
        frame
            .scroll
            .place(element, Point::new(DevicePx(0.0), DevicePx(by)));
        compose(&mut frame, &mut store, &mut Scrolled { node: element });
        let placements = Placements::of(&frame.spatial);
        let piece = |key| {
            let fragment = store
                .fragment(store.fragments_of_box(key)[0])
                .expect("a live piece");
            onto_device(fragment.border_box, fragment.transform, &placements)
        };
        let port_piece = store
            .fragment(store.fragments_of_box(port_key)[0])
            .expect("a port piece");
        let port_top = onto_device(port_piece.padding_box, port_piece.transform, &placements)
            .origin
            .y
            .0;
        assert_eq!(
            piece(top).origin.y.0,
            port_top,
            "scrolled to {by}, the port's header is not at the port's top edge"
        );
    }
}

#[test]
fn a_scroll_past_a_sticky_header_visits_the_header_and_no_row() {
    let _guard = exclusive();
    let page =
        |rows: usize| {
            let mut children = vec![Element::new("top").text("delta gamma")];
            children.extend((0..rows).map(|_| {
                Element::new("row").children(vec![Element::new("card").text("alpha bravo")])
            }));
            Fixture::new(
                Element::new("root").children(vec![Element::new("port").children(children)]),
                "root { display: block; width: 300px; height: 240px }
             port { display: block; height: 200px; overflow: scroll; margin-top: 20px }
             top  { display: block; height: 16px; position: sticky; top: 0px }
             row  { display: block; height: 20px }
             card { display: block; overflow: hidden; border-radius: 4px; height: 18px }",
            )
        };
    let fixture = page(1000);
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 300.0, 240.0);
    let element = port(&store);
    for by in [40.0, 60.0] {
        frame
            .scroll
            .place(element, Point::new(DevicePx(0.0), DevicePx(by)));
        counter::reset();
        compose(&mut frame, &mut store, &mut Scrolled { node: element });
    }
    let visited = counter::get(Counter::NodesVisited);
    assert!(scrolls_content(&store, port_box(&store)));
    if !COUNTERS_ENABLED {
        return;
    }
    assert!(
        visited <= 8,
        "a scroll visited {visited} boxes; the path down to the port and the header is a handful"
    );
}

/// A positioned scrollport holding a fixed box of its own and, in one positioned row, a fixed
/// box positioned against that row.
fn fixed_page() -> Fixture {
    let mut rows: Vec<Element> = (0..12)
        .map(|_| Element::new("row").children(vec![Element::new("card").text("alpha bravo")]))
        .collect();
    rows[3] = Element::new("held").children(vec![
        Element::new("card").text("alpha bravo"),
        Element::new("badge").text("delta"),
    ]);
    rows.push(Element::new("fab").text("gamma"));
    Fixture::new(
        Element::new("root").children(vec![Element::new("port").children(rows)]),
        "root  { display: block; width: 300px; height: 240px }
         port  { display: block; position: relative; height: 200px; overflow: scroll;
                 margin-top: 20px }
         row   { display: block; height: 20px }
         held  { display: block; height: 20px; position: relative }
         card  { display: block; overflow: hidden; border-radius: 4px; height: 18px }
         fab   { display: block; position: fixed; top: 150px; left: 200px; width: 40px;
                 height: 30px }
         badge { display: block; position: fixed; top: 4px; left: 250px; width: 20px;
                 height: 10px }",
    )
}

#[test]
fn a_fixed_box_inside_a_content_space_stays_where_a_fresh_build_puts_it() {
    let _guard = exclusive();
    let (spaced, fresh, held) = two_ways(&fixed_page, &STEPS);
    assert!(
        held.iter().all(|held| *held),
        "the port gave its content space up over fixed boxes: {held:?}"
    );
    agree(&spaced, &fresh);

    // Both fixed boxes stay where they were drawn before any scroll, and the damage of a scroll
    // that writes the content space holds them where they are and where moving the port's
    // pixels would put them.
    let fixture = fixed_page();
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 300.0, 240.0);
    let element = port(&store);
    let port_key = port_box(&store);
    let fixed: Vec<zgui_layout::BoxKey> = store
        .keys()
        .into_iter()
        .filter(|&key| {
            store
                .fragments_of_box(key)
                .first()
                .and_then(|frag| store.fragment(*frag))
                .is_some_and(|piece| piece.flags.contains(FragmentFlags::FIXED_BELOW))
                && store.node(key).children.iter().all(|child| {
                    store
                        .fragments_of_box(*child)
                        .first()
                        .and_then(|frag| store.fragment(*frag))
                        .is_none_or(|piece| !piece.flags.contains(FragmentFlags::FIXED_BELOW))
                })
                && key != port_key
        })
        .collect();
    let device = |store: &LayoutStore, frame: &Frame, key| {
        let fragment = store
            .fragment(store.fragments_of_box(key)[0])
            .expect("a live piece");
        onto_device(
            fragment.border_box,
            fragment.transform,
            &Placements::of(&frame.spatial),
        )
    };
    let standing: Vec<_> = fixed
        .iter()
        .map(|key| device(&store, &frame, *key))
        .collect();
    assert_eq!(standing.len(), 2, "the page has two fixed boxes: {fixed:?}");
    let mut last = 0.0;
    for by in STEPS {
        frame
            .scroll
            .place(element, Point::new(DevicePx(0.0), DevicePx(by)));
        let moves = compose(&mut frame, &mut store, &mut Scrolled { node: element });
        let placements = Placements::of(&frame.spatial);
        for (key, was) in fixed.iter().zip(&standing) {
            let now = device(&store, &frame, *key);
            assert_eq!(now, *was, "scrolled to {by}, a fixed box moved");
            // The paint walk skips a subtree by its folded ink, so every box above a fixed box
            // folds the fixed box's ink where it is drawn.
            let ink = store
                .fragment(store.fragments_of_box(*key)[0])
                .expect("a live piece")
                .device_ink(&placements);
            let mut above = store.node(*key).parent;
            while let Some(parent) = above {
                let folded = store
                    .fragment(store.fragments_of_box(parent)[0])
                    .expect("a live piece")
                    .device_subtree_ink(&placements);
                assert!(
                    folded.contains_rect(ink),
                    "scrolled to {by}, box {parent:?} folds {folded:?}, which leaves the fixed \
                     box's ink {ink:?} out"
                );
                above = store.node(parent).parent;
            }
            if moves.by.is_some() {
                let shifted =
                    now.translate(zgui_geom::Size::new(DevicePx(0.0), DevicePx(last - by)));
                // What the port's clip admits of each is what can be drawn there.
                let port_piece = store
                    .fragment(store.fragments_of_box(port_key)[0])
                    .expect("a port piece");
                let port_rect =
                    onto_device(port_piece.padding_box, port_piece.transform, &placements);
                for rect in [now, shifted] {
                    let Some(rect) = rect.intersection(port_rect) else {
                        continue;
                    };
                    let pixels = zgui_layout::fragment::diff::pixels(rect);
                    assert!(
                        moves
                            .beyond
                            .rects()
                            .iter()
                            .any(|held| held.contains_rect(pixels)),
                        "scrolled to {by}, the damage beyond the move {:?} leaves {pixels:?} out",
                        moves.beyond.rects()
                    );
                }
            }
        }
        last = by;
    }
}

/// The scrollport of [`page`] with transformed rows, and optionally a fixed box inside one of
/// them.
fn transformed_page(with_fixed: bool) -> Fixture {
    let mut rows: Vec<Element> = (0..12)
        .map(|_| Element::new("row").children(vec![Element::new("card").text("alpha bravo")]))
        .collect();
    if with_fixed {
        rows[2] = Element::new("row").children(vec![
            Element::new("card").text("alpha bravo"),
            Element::new("badge").text("delta"),
        ]);
    }
    Fixture::new(
        Element::new("root").children(vec![Element::new("port").children(rows)]),
        "root  { display: block; width: 300px; height: 240px }
         port  { display: block; height: 200px; overflow: scroll; margin-top: 20px }
         row   { display: block; height: 20px; position: relative;
                 transform: translate(3px, 1px) }
         card  { display: block; overflow: hidden; border-radius: 4px; height: 18px }
         badge { display: block; position: fixed; top: 2px; left: 250px; width: 20px;
                 height: 10px }",
    )
}

#[test]
fn transformed_rows_take_a_content_space_and_are_drawn_where_a_fresh_build_puts_them() {
    let _guard = exclusive();
    let (spaced, fresh, held) = two_ways(&|| transformed_page(false), &STEPS);
    assert!(
        held.iter().all(|held| *held),
        "the port gave its content space up over transformed rows: {held:?}"
    );
    agree(&spaced, &fresh);
}

#[test]
fn a_fixed_box_in_a_transformed_row_keeps_the_port_out_of_a_content_space() {
    let _guard = exclusive();
    let (spaced, fresh, held) = two_ways(&|| transformed_page(true), &STEPS);
    assert!(
        held.iter().all(|held| !held),
        "the port took a content space that would carry the row's fixed box: {held:?}"
    );
    agree(&spaced, &fresh);
}

#[test]
fn a_new_port_with_a_fixed_box_in_a_transformed_row_takes_no_content_space() {
    let _guard = exclusive();
    let fixture = transformed_page(true);
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let frame = lay_out(&mut store, &mut content, 300.0, 240.0);
    let key = port_box(&store);
    assert!(
        !scrolls_content(&store, key),
        "the new port took a content space that would carry the row's fixed box"
    );
    assert!(
        frame
            .spatial
            .of(zgui_scene::PropertyOwner::of(key).content())
            .is_none(),
        "the new port kept the content space it gave back"
    );
}

#[test]
fn a_content_space_draws_every_row_where_a_port_kept_out_of_one_draws_it() {
    let _guard = exclusive();
    // The same rows, scrolled the same way, once in a content space and once in the port's own
    // space, which a fixed box in one row keeps the port in. The fixed box is left out of the
    // comparison.
    let run = |with_fixed: bool| -> (Vec<Vec<Drawn>>, Vec<bool>) {
        let fixture = transformed_page(with_fixed);
        let mut store = fixture.box_tree();
        let mut content = measurer();
        let mut frame = lay_out(&mut store, &mut content, 300.0, 240.0);
        let element = port(&store);
        let mut skip = Vec::new();
        if with_fixed {
            let row = store.node(port_box(&store)).children[2];
            let mut stack = vec![store.node(row).children[1]];
            while let Some(key) = stack.pop() {
                skip.push(key);
                stack.extend_from_slice(&store.node(key).children);
            }
        }
        let mut drawn = vec![drawn_except(&frame, &store, &skip)];
        let mut held = vec![scrolls_content(&store, port_box(&store))];
        for by in STEPS {
            frame
                .scroll
                .place(element, Point::new(DevicePx(0.0), DevicePx(by)));
            compose(&mut frame, &mut store, &mut Scrolled { node: element });
            drawn.push(drawn_except(&frame, &store, &skip));
            held.push(scrolls_content(&store, port_box(&store)));
        }
        (drawn, held)
    };
    let (spaced, spaced_held) = run(false);
    let (kept, kept_held) = run(true);
    assert!(
        spaced_held.iter().all(|held| *held),
        "the port without a fixed box gave its content space up: {spaced_held:?}"
    );
    assert!(
        kept_held.iter().all(|held| !held),
        "the port with a fixed box took a content space: {kept_held:?}"
    );
    for (step, (spaced, kept)) in spaced.iter().zip(&kept).enumerate() {
        assert!(spaced.len() > 30, "too few fragments to compare");
        assert_eq!(
            spaced, kept,
            "at step {step}, the content space draws a row somewhere the port's own space does not"
        );
    }
}

/// The scrollport of [`page`] with one row that holds a sticky box, and optionally transformed
/// rows and a fixed box inside the sticky box.
fn sticky_in_row_page(transformed: bool, with_fixed: bool) -> Fixture {
    let mut rows: Vec<Element> = (0..12)
        .map(|_| Element::new("row").children(vec![Element::new("card").text("alpha bravo")]))
        .collect();
    let mut inner = vec![Element::new("card").text("alpha bravo")];
    if with_fixed {
        inner.push(Element::new("badge").text("delta"));
    }
    rows[2] = Element::new("row").children(vec![Element::new("wrap").children(inner)]);
    let transform = if transformed {
        "transform: translate(3px, 1px)"
    } else {
        ""
    };
    Fixture::new(
        Element::new("root").children(vec![Element::new("port").children(rows)]),
        &format!(
            "root  {{ display: block; width: 300px; height: 240px }}
             port  {{ display: block; height: 200px; overflow: scroll; margin-top: 20px }}
             row   {{ display: block; height: 20px; position: relative; {transform} }}
             wrap  {{ display: block; height: 10px; position: sticky; top: 0px }}
             card  {{ display: block; overflow: hidden; border-radius: 4px; height: 8px }}
             badge {{ display: block; position: fixed; top: 2px; left: 250px; width: 20px;
                     height: 10px }}"
        ),
    )
}

#[test]
fn a_sticky_box_in_a_transformed_row_takes_a_content_space_and_is_drawn_where_a_fresh_build_puts_it()
 {
    let _guard = exclusive();
    let (spaced, fresh, held) = two_ways(&|| sticky_in_row_page(true, false), &STEPS);
    assert!(
        held.iter().all(|held| *held),
        "the port gave its content space up over a sticky box in a transformed row: {held:?}"
    );
    agree(&spaced, &fresh);
}

#[test]
fn a_fixed_box_in_a_sticky_box_keeps_the_port_out_of_a_content_space() {
    let _guard = exclusive();
    for transformed in [false, true] {
        let (spaced, fresh, held) = two_ways(&|| sticky_in_row_page(transformed, true), &STEPS);
        assert!(
            held.iter().all(|held| !held),
            "transformed rows {transformed}: the port took a content space that would carry the \
             sticky box's fixed box: {held:?}"
        );
        agree(&spaced, &fresh);
    }
}

#[test]
fn a_row_that_layout_moves_as_its_port_scrolls_damages_where_the_moved_pixels_put_it() {
    let _guard = exclusive();
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
    let port_key = port_box(&store);
    assert!(scrolls_content(&store, port_key));
    let rows = store.node(port_key).children.clone();
    let device_of = |store: &LayoutStore, frame: &Frame, key| {
        store
            .fragment(store.fragments_of_box(key)[0])
            .expect("a live piece")
            .device_subtree_ink(&Placements::of(&frame.spatial))
    };
    let before: Vec<_> = rows[4..9]
        .iter()
        .map(|key| device_of(&store, &frame, *key))
        .collect();

    // Row 3 grows, so layout moves every row after it, and the port scrolls in the same frame.
    let target = store
        .node(rows[3])
        .source
        .expect("the row came from an element");
    let index = fixture
        .document
        .store()
        .index_of(target)
        .expect("a live element");
    fixture.edit_restyle_and_patch(&mut store, |edit| {
        edit.set_style_property(index, "height", Some("30px"));
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
    frame
        .scroll
        .place(element, Point::new(DevicePx(0.0), DevicePx(55.0)));
    let mut dirty = Owing {
        target,
        above,
        bits: Dirty::RELAYOUT | Dirty::REPAINT,
        scrolled: Some(element),
    };
    let moves = compose(&mut frame, &mut store, &mut dirty);
    assert!(scrolls_content(&store, port_key));
    let by = moves.by.expect("the frame reports the port's move");
    let placements = Placements::of(&frame.spatial);
    let port_piece = store
        .fragment(store.fragments_of_box(port_key)[0])
        .expect("a port piece");
    let port_rect = onto_device(port_piece.padding_box, port_piece.transform, &placements);
    for (key, was) in rows[4..9].iter().zip(before) {
        assert_ne!(
            device_of(&store, &frame, *key),
            was.translate(zgui_geom::Size::new(DevicePx(by.0), DevicePx(by.1))),
            "row {key:?} moved with the port only, so layout did not move it"
        );
        // Moving the port's pixels puts the row's old image here, so it is drawn again.
        let carried = was.translate(zgui_geom::Size::new(DevicePx(by.0), DevicePx(by.1)));
        let Some(rect) = carried.intersection(port_rect) else {
            continue;
        };
        let pixels = zgui_layout::fragment::diff::pixels(rect);
        assert!(
            moves
                .beyond
                .rects()
                .iter()
                .any(|held| held.contains_rect(pixels)),
            "row {key:?}: the damage beyond the move {:?} leaves {pixels:?} out",
            moves.beyond.rects()
        );
    }
}

#[test]
fn a_port_kept_out_of_a_content_space_damages_every_row_it_carries_where_it_was_and_where_it_is() {
    let _guard = exclusive();
    // A fixed box in a sticky box keeps the port out of a content space, so a scroll changes the
    // shift the rows are composed against. The rows are carried as layout moves: the frame cannot
    // move the port's pixels, so each row is damaged where it was and where it is.
    let fixture = sticky_in_row_page(false, true);
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 300.0, 240.0);
    let element = port(&store);
    let port_key = port_box(&store);
    frame
        .scroll
        .place(element, Point::new(DevicePx(0.0), DevicePx(30.0)));
    compose(&mut frame, &mut store, &mut Scrolled { node: element });
    let rows = store.node(port_key).children.clone();
    let device_of = |store: &LayoutStore, frame: &Frame, key| {
        store
            .fragment(store.fragments_of_box(key)[0])
            .expect("a live piece")
            .device_subtree_ink(&Placements::of(&frame.spatial))
    };
    let before: Vec<_> = rows
        .iter()
        .map(|key| device_of(&store, &frame, *key))
        .collect();

    frame
        .scroll
        .place(element, Point::new(DevicePx(0.0), DevicePx(50.0)));
    frame.damage = zgui_bits::DamageSet::new();
    counter::reset();
    let moves = compose(&mut frame, &mut store, &mut Scrolled { node: element });
    let shifted = counter::get(Counter::BoxesShifted);
    assert!(
        !scrolls_content(&store, port_key),
        "the port took a content space that would carry the sticky box's fixed box"
    );
    assert_eq!(
        (moves.count, moves.by),
        (0, None),
        "the scroll reported a move the frame could answer by moving pixels"
    );
    assert_eq!(
        drawn(&frame, &store),
        fresh(&fixture, element, 50.0),
        "the scrolled port draws a piece somewhere a fresh build does not"
    );
    let placements = Placements::of(&frame.spatial);
    let port_piece = store
        .fragment(store.fragments_of_box(port_key)[0])
        .expect("a port piece");
    let port_rect = onto_device(port_piece.padding_box, port_piece.transform, &placements);
    let mut checked = 0;
    // Row 2 holds the sticky box, which does not move with the rows.
    for (index, (key, was)) in rows.iter().zip(before).enumerate() {
        if index == 2 {
            continue;
        }
        let is = device_of(&store, &frame, *key);
        assert_eq!(
            is,
            was.translate(zgui_geom::Size::new(DevicePx(0.0), DevicePx(-20.0))),
            "row {key:?} did not move by the change of offset"
        );
        for rect in [was, is] {
            let Some(rect) = rect.intersection(port_rect) else {
                continue;
            };
            let pixels = zgui_layout::fragment::diff::pixels(rect);
            assert!(
                moves
                    .beyond
                    .rects()
                    .iter()
                    .any(|held| held.contains_rect(pixels)),
                "row {key:?}: the damage beyond the move {:?} leaves {pixels:?} out",
                moves.beyond.rects()
            );
            checked += 1;
        }
    }
    assert!(
        checked > 10,
        "only {checked} row rectangles were in the port"
    );
    if COUNTERS_ENABLED {
        assert!(
            shifted >= 10,
            "{shifted} boxes were carried; the scroll should have carried the plain rows"
        );
    }
}
