//! What answers at a point: painting order, pointer events, clips, transforms and deletion.

use zgui_geom::{Device, DevicePx, Point};

use crate::probe::box_named;
use crate::support::{Element, Fixture, fragments, lay_out, measurer};

#[test]
fn the_topmost_fragment_under_a_point_is_the_one_painted_last() {
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("under"), Element::new("over")]),
        "root { display: block; width: 200px; position: relative }
         under { display: block; height: 50px }
         over { display: block; height: 50px; position: absolute; top: 0; left: 0; width: 200px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let frame = lay_out(&mut store, &mut content, 200.0, 200.0);

    let over = box_named(&store, &fixture, "over");
    let hits = frame.hit.hit(
        Point::new(DevicePx(20.0), DevicePx(20.0)),
        &frame.clips,
        &frame.spatial,
    );
    assert!(!hits.is_empty());
    let top = store.fragment(hits[0]).expect("live");
    assert_eq!(top.box_, over, "the positioned box is painted last");
}

#[test]
fn a_later_positioned_sibling_answers_over_an_earlier_absolute_box() {
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("a").children(vec![Element::new("r"), Element::new("c")]),
        ]),
        "root { display: block; width: 200px }
         a { display: block; position: relative; height: 40px }
         r { display: block; position: absolute; top: 0; left: 0; right: 0; height: 40px }
         c { display: block; position: relative; height: 40px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let frame = lay_out(&mut store, &mut content, 200.0, 200.0);

    let c = box_named(&store, &fixture, "c");
    let hits = frame.hit.hit(
        Point::new(DevicePx(20.0), DevicePx(20.0)),
        &frame.clips,
        &frame.spatial,
    );
    let top = store.fragment(hits[0]).expect("live");
    assert_eq!(top.box_, c, "the sibling written later is painted last");
}

#[test]
fn a_fragment_that_takes_no_pointer_events_is_not_an_answer() {
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("under"), Element::new("ghost")]),
        "root { display: block; width: 200px; position: relative }
         under { display: block; height: 50px }
         ghost { display: block; height: 50px; position: absolute; top: 0; left: 0;
                 width: 200px; pointer-events: none }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let frame = lay_out(&mut store, &mut content, 200.0, 200.0);

    let ghost = box_named(&store, &fixture, "ghost");
    let hits = frame.hit.hit(
        Point::new(DevicePx(20.0), DevicePx(20.0)),
        &frame.clips,
        &frame.spatial,
    );
    for hit in &hits {
        assert_ne!(
            store.fragment(*hit).expect("live").box_,
            ghost,
            "an overlay that takes no events must not swallow the click"
        );
    }
    assert!(!hits.is_empty(), "the box under it still answers");
}

#[test]
fn a_point_outside_a_scrollport_misses_what_is_inside_it() {
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("port").children(vec![Element::new("row")]),
        ]),
        "root { display: block; width: 300px }
         port { display: block; height: 40px; overflow: hidden }
         row { display: block; height: 400px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let frame = lay_out(&mut store, &mut content, 300.0, 300.0);

    let row = box_named(&store, &fixture, "row");
    let inside: Point<DevicePx, Device> = Point::new(DevicePx(10.0), DevicePx(20.0));
    let below = Point::new(DevicePx(10.0), DevicePx(200.0));
    let hits_inside = frame.hit.hit(inside, &frame.clips, &frame.spatial);
    assert!(
        hits_inside
            .iter()
            .any(|hit| store.fragment(*hit).expect("live").box_ == row),
        "the row answers where the scrollport shows it"
    );
    let hits_below = frame.hit.hit(below, &frame.clips, &frame.spatial);
    assert!(
        !hits_below
            .iter()
            .any(|hit| store.fragment(*hit).expect("live").box_ == row),
        "and not where the scrollport has cut it off"
    );
}

#[test]
fn a_transformed_fragment_is_hit_where_it_appears() {
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("moved")]),
        "root { display: block; width: 300px }
         moved { display: block; height: 40px; transform: translateX(100px) }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let frame = lay_out(&mut store, &mut content, 300.0, 300.0);

    let moved = box_named(&store, &fixture, "moved");
    let hits = |x: f32| {
        frame
            .hit
            .hit(
                Point::new(DevicePx(x), DevicePx(20.0)),
                &frame.clips,
                &frame.spatial,
            )
            .iter()
            .any(|hit| store.fragment(*hit).expect("live").box_ == moved)
    };
    assert!(hits(150.0), "where the transform put it");
    assert!(!hits(20.0), "and not where layout did");
}

#[test]
fn a_transformed_box_answers_only_where_its_ancestors_clip_shows_it() {
    // A clip belongs to the box that imposed it and is measured in *that* box's space; a
    // descendant that carries a transform of its own is in a different space entirely. Testing the
    // clip in the descendant's space puts the scrollport wherever the transform put the
    // descendant, so a box translated clean out of its scrollport goes on answering clicks over
    // the empty screen it was translated onto.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("port").children(vec![Element::new("moved")]),
        ]),
        "root { display: block; width: 400px }
         port { display: block; width: 100px; height: 100px; overflow: hidden }
         moved { display: block; height: 50px; transform: translateX(200px) }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let frame = lay_out(&mut store, &mut content, 400.0, 400.0);

    let moved = box_named(&store, &fixture, "moved");
    let answers = |x: f32| {
        frame
            .hit
            .hit(
                Point::new(DevicePx(x), DevicePx(20.0)),
                &frame.clips,
                &frame.spatial,
            )
            .iter()
            .any(|hit| store.fragment(*hit).expect("live").box_ == moved)
    };
    assert!(
        !answers(250.0),
        "the transform put it outside the scrollport, so nothing of it is on the screen"
    );
    assert!(!answers(50.0), "and it is not where layout put it either");

    // The control: the same box translated by less than the scrollport is wide is still visible,
    // and is hit where the transform put it and nowhere else.
    let visible = Fixture::new(
        Element::new("root").children(vec![
            Element::new("port").children(vec![Element::new("moved")]),
        ]),
        "root { display: block; width: 400px }
         port { display: block; width: 300px; height: 100px; overflow: hidden }
         moved { display: block; width: 50px; height: 50px; transform: translateX(100px) }",
    );
    let mut store = visible.box_tree();
    let mut content = measurer();
    let frame = lay_out(&mut store, &mut content, 400.0, 400.0);
    let moved = box_named(&store, &visible, "moved");
    let answers = |x: f32| {
        frame
            .hit
            .hit(
                Point::new(DevicePx(x), DevicePx(20.0)),
                &frame.clips,
                &frame.spatial,
            )
            .iter()
            .any(|hit| store.fragment(*hit).expect("live").box_ == moved)
    };
    assert!(answers(120.0), "still inside the port, so still clickable");
    assert!(!answers(20.0));
}

/// Whether the box named `leaf` answers at `(x, y)` in a port that clips a transformed box.
///
/// `port` clips in the viewport's space. `turned` carries `transform`, and clips in its own space
/// when `turned_clips` is set. `leaf` fills `turned`, so its clip chain names a space other than
/// its own.
fn leaf_under_turned_clip(
    port: &str,
    transform: &str,
    turned_clips: bool,
) -> impl Fn(f32, f32) -> bool {
    let overflow = if turned_clips { "hidden" } else { "visible" };
    let css = format!(
        "root {{ display: block; width: 400px }}
         port {{ display: block; {port}; overflow: hidden }}
         turned {{ display: block; width: 150px; height: 50px; overflow: {overflow};
                   transform-origin: 0 0; transform: {transform} }}
         leaf {{ display: block; width: 150px; height: 50px }}"
    );
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("port").children(vec![
            Element::new("turned").children(vec![Element::new("leaf")]),
        ])]),
        &css,
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let frame = lay_out(&mut store, &mut content, 400.0, 400.0);
    let leaf = box_named(&store, &fixture, "leaf");
    move |x, y| {
        frame
            .hit
            .hit(
                Point::new(DevicePx(x), DevicePx(y)),
                &frame.clips,
                &frame.spatial,
            )
            .iter()
            .any(|hit| store.fragment(*hit).expect("live").box_ == leaf)
    }
}

#[test]
fn a_clip_outside_a_scaled_clipping_box_is_tested_in_its_own_space() {
    // `turned` doubles its 150px box to 300px on the device and clips its child to that. The port
    // clips at 200px in the viewport's space. Tested in `turned`'s space, the port's clip would
    // reach out to 400px on the device.
    let answers = leaf_under_turned_clip("width: 200px; height: 200px", "scale(2)", true);
    assert!(answers(10.0, 10.0), "inside both clips");
    assert!(answers(199.5, 90.0), "just inside the port's edge");
    assert!(!answers(200.5, 90.0), "just outside the port's edge");
    assert!(
        !answers(250.0, 50.0),
        "drawn by `turned`, cut away by the port"
    );
    assert!(!answers(150.0, 100.5), "below `turned`, scaled with it");
}

#[test]
fn a_clip_outside_a_scaled_box_that_does_not_clip_is_tested_in_its_own_space() {
    // `turned` doubles its 150px box to 300px on the device and clips nothing. The leaf's only
    // link is the port's, which clips at 200px in the viewport's space. Tested in `turned`'s
    // space, the port's clip would reach out to 400px on the device.
    let answers = leaf_under_turned_clip("width: 200px; height: 200px", "scale(2)", false);
    assert!(answers(10.0, 10.0), "inside the port");
    assert!(answers(199.5, 90.0), "just inside the port's edge");
    assert!(!answers(200.5, 90.0), "just outside the port's edge");
    assert!(
        !answers(250.0, 50.0),
        "drawn by `turned`, cut away by the port"
    );
}

#[test]
fn a_clip_outside_a_rotated_clipping_box_is_tested_in_its_own_space() {
    // `turned` is turned a quarter and moved 100px right, so its 150 by 50 box is drawn over
    // x 50..100 and y 0..150. The port is 100px wide and 120px tall, so it cuts the box at
    // y = 120. Tested in `turned`'s space, where y on the device is x, the port's 100px width
    // would cut it at y = 100 and miss a strip that is on the screen.
    let answers = leaf_under_turned_clip(
        "width: 100px; height: 120px",
        "translate(100px, 0) rotate(90deg)",
        true,
    );
    assert!(answers(75.0, 10.0), "inside both clips");
    assert!(
        answers(75.0, 110.0),
        "on the strip the port's own width does not reach"
    );
    assert!(answers(75.0, 119.5), "just inside the port's bottom edge");
    assert!(!answers(75.0, 120.5), "just outside the port's bottom edge");
    assert!(!answers(49.5, 60.0), "outside `turned`, turned with it");
}

#[test]
fn a_box_taken_out_of_the_tree_stops_answering_where_it_used_to_be() {
    // The walk that keeps the index in step visits boxes that are still in the tree, so a box that
    // was deleted is the one case it can never reach. Its entries have to be unregistered by the
    // fact that its pieces were destroyed — otherwise a deleted row answers clicks for ever, in
    // front of whatever moved up into its place.
    let rows: Vec<Element> = (0..60).map(|_| Element::new("row")).collect();
    let fixture = Fixture::new(
        Element::new("root").children(rows),
        "root { display: block; width: 200px }
         row { display: block; height: 10px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 200.0, 600.0);
    let indexed = frame.hit.len();

    let root = store.root().expect("a root");
    let victim = store.node(root).children[3];
    let gone: Vec<_> = store.fragments_of_box(victim).to_vec();
    assert!(!gone.is_empty());
    zgui_layout::boxtree::patch::detach(&mut store, victim);
    {
        let mut tree = zgui_layout::tree::LayoutTree::new(
            &mut store,
            &mut content,
            zgui_layout::DeviceStyle::default(),
        );
        assert!(tree.layout_root(cephal::Size {
            width: 200.0,
            height: 600.0
        }));
    }
    // A frame in which nothing else is dirty, which is the case that cannot be rescued by the
    // index deciding to rebuild itself: a small enough change never reaches the churn threshold.
    fragments(&mut frame, &mut store, root, &mut Clean);

    assert_eq!(
        frame.hit.len(),
        indexed - gone.len(),
        "the deleted row's pieces are still indexed"
    );
    for frag in &gone {
        assert!(frame.hit.entry(*frag).is_none());
    }
    store.recycle();
    for hit in frame.hit.hit(
        Point::new(DevicePx(10.0), DevicePx(35.0)),
        &frame.clips,
        &frame.spatial,
    ) {
        assert!(
            store.fragment(hit).is_some(),
            "a hit resolved to a fragment that no longer exists"
        );
    }
}

/// A document nothing has marked, which is what a frame that only re-composes looks like.
pub(crate) struct Clean;

impl zgui_layout::fragment::diff::FrameDirty for Clean {
    fn own(&self, _node: Option<zgui_dom::NodeKey>) -> zgui_bits::Dirty {
        zgui_bits::Dirty::empty()
    }

    fn subtree(&self, _node: Option<zgui_dom::NodeKey>) -> zgui_bits::Dirty {
        zgui_bits::Dirty::empty()
    }

    fn mark(&mut self, _node: Option<zgui_dom::NodeKey>, _bits: zgui_bits::Dirty) {}

    fn retire(&mut self, _node: Option<zgui_dom::NodeKey>, _phase: zgui_bits::Dirty) {
        // Nothing is marked, so nothing can be retired.
    }
}

#[test]
fn a_scrollbar_inside_a_transformed_scroller_answers_where_it_is_drawn() {
    // A box's own piece is the only piece the fragment pass gives a device rectangle to: a line, a
    // run and a scrollbar are placed inside their box and keep the box's space. Under one hierarchy
    // over device rectangles those pieces were filed at their untransformed positions, so a bar in a
    // translated scroller answered nowhere at all — not where it is drawn, because the entry was
    // never there, and not where layout put it, because the clip chain that reaches it is resolved
    // in the space that did move.
    //
    // Grouped by space there is nothing to reconcile: every rectangle in one tree is in one space,
    // and the point is brought down to meet them.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("moved").children(vec![Element::new("tall")]),
        ]),
        "root { display: block; width: 400px }
         moved { display: block; width: 100px; height: 100px; overflow: scroll;
                 transform: translateX(200px) }
         tall { display: block; height: 400px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let frame = lay_out(&mut store, &mut content, 400.0, 400.0);

    let bar = |x: f32| {
        frame
            .hit
            .hit(
                Point::new(DevicePx(x), DevicePx(50.0)),
                &frame.clips,
                &frame.spatial,
            )
            .iter()
            .filter_map(|hit| store.fragment(*hit))
            .any(|fragment| matches!(fragment.kind, zgui_layout::FragmentKind::Scrollbar { .. }))
    };
    assert!(bar(292.0), "the bar is where the transform put it");
    assert!(!bar(92.0), "and not where layout put it");
}
