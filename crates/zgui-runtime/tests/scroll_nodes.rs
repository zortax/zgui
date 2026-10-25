//! What a scroll of a long list costs when its port draws the rows in a content space.
//!
//! The claim: a scroll frame visits the path down to the port and moves no hit entry and no
//! accessibility rectangle of any row. The port takes its content space when it is first composed,
//! so the first scroll is no exception. The rows still have to end up where the scroll put them,
//! for the screen, for the pointer and for an assistive technology, so the test reads all three
//! back.
//!
//! Its own test target, because the counters are process-wide.

mod support;

use zgui_a11y::{NodeId, TreeId};
use zgui_geom::{Css, CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_platform_headless::Harness;
use zgui_profile::counter;
use zgui_runtime::Runtime;
use zgui_view::{A11yBinding, IntoView, NodeRef, View};
use zgui_vocab::{Modifiers, Role, ScrollDelta, ScrollPhase, Timestamp, WheelEvent};

/// A port much shorter than the rows in it.
const CSS: &str = "
root { display: block; width: 400px; height: 300px; background-color: #ffffff }
.port { display: block; width: 400px; height: 240px; overflow: scroll;
        background-color: #101010 }
.row { display: block; width: 360px; height: 20px; background-color: #202020 }
";

/// How many rows the list holds, none of them virtualised.
const ROWS: usize = 1000;

/// How tall one row is, in device pixels.
const ROW: f32 = 20.0;

/// How many frames each wheel turn is carried for.
const FRAMES: usize = 24;

/// A window holding the list, with `watched` bound to the row at `index`.
fn listing(watched: NodeRef, index: usize) -> Harness<Runtime> {
    support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        let mut port = zgui_elements::column().class("port");
        for row in 0..ROWS {
            let mut item = zgui_elements::control()
                .class("row")
                .a11y(A11yBinding::new(Role::Button).label("Open"));
            if row == index {
                item = item.node_ref(watched);
            }
            port = port.child(item);
        }
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(port)
                .into_view()
                .build(cx),
        )
    })
}

/// Turns the wheel `lines` lines over the port, and carries the turn to its end.
fn wheel(harness: &mut Harness<Runtime>, lines: f32) {
    harness.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(120.0)),
            delta: ScrollDelta::Lines { x: 0.0, y: lines },
            phase: ScrollPhase::Discrete,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    for _ in 0..FRAMES {
        harness.advance(std::time::Duration::from_millis(20));
        harness.pump();
    }
    harness.settle(4);
}

/// The watched row's border box where the frame drew it.
fn drawn(
    harness: &Harness<Runtime>,
    watched: NodeRef,
) -> zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device> {
    let window = harness.app().windows().first().expect("a window");
    let layout = window.layout().borrow();
    let element = element_of(watched);
    let frag = *layout
        .fragments_of(element)
        .first()
        .expect("the row has a piece");
    let fragment = layout.fragment(frag).expect("a live piece");
    zgui_layout::fragment::transform::placed::onto_device(
        fragment.border_box,
        fragment.transform,
        &window.placements(),
    )
}

/// The document's name for the element `watched` is bound to.
fn element_of(watched: NodeRef) -> zgui_dom::NodeKey {
    let node = watched.get_untracked().expect("the row is bound");
    zgui_view_dom::id::to_document(node).expect("a live element")
}

/// What one measured scroll cost, and where the watched row was before and after it.
struct Scrolled {
    /// The counter deltas over the measured scroll.
    cost: zgui_profile::Counters,
    /// The watched row's top before the scroll.
    before: f32,
    /// The harness after the scroll.
    harness: Harness<Runtime>,
}

/// Measures a scroll of three lines, after a scroll of one line when `warm` is set.
fn scroll(watched: NodeRef, warm: bool) -> Scrolled {
    let mut harness = listing(watched, 16);
    harness.settle(8);
    if warm {
        wheel(&mut harness, 1.0);
    }
    let before = drawn(&harness, watched).origin.y.0;
    let start = counter::snapshot();
    wheel(&mut harness, 3.0);
    let cost = start.delta(&counter::snapshot());
    Scrolled {
        cost,
        before,
        harness,
    }
}

#[test]
fn a_scroll_of_a_long_list_visits_the_path_down_and_moves_no_row_entry() {
    let _turn = counter::exclusive();
    for warm in [true, false] {
        let watched = NodeRef::new();
        let spaced = scroll(watched, warm);
        assert_cheap(&spaced, watched, warm);
    }
}

/// Asserts that the measured scroll of `spaced` moved the watched row and visited, updated and
/// measured no more than the path down to the port.
fn assert_cheap(spaced: &Scrolled, watched: NodeRef, warm: bool) {
    let scroll = if warm {
        "a later scroll"
    } else {
        "the first scroll"
    };
    let after = drawn(&spaced.harness, watched).origin.y.0;
    assert!(
        after < spaced.before - ROW,
        "{scroll}: the row did not move: {} to {after}. A scroll that moved nothing satisfies \
         every count below",
        spaced.before
    );
    if !zgui_profile::COUNTERS_ENABLED {
        return;
    }
    let passes = spaced.cost.layout_passes.max(1);
    assert!(
        spaced.cost.fragment_boxes_visited <= 4 * passes,
        "{scroll}: the fragment pass reached {} boxes over {passes} walks: more than the path \
         down to the port",
        spaced.cost.fragment_boxes_visited
    );
    assert!(
        spaced.cost.hit_entries_updated <= 2 * passes,
        "{scroll}: {} hit entries updated over {passes} walks: more than the port's own bar pieces",
        spaced.cost.hit_entries_updated
    );
    assert!(
        spaced.cost.a11y_remeasured <= 2 * passes,
        "{scroll}: {} accessibility rectangles measured over {passes} walks: more than the port's \
         own",
        spaced.cost.a11y_remeasured
    );
}

#[test]
fn a_row_in_a_content_space_is_hit_and_reported_where_it_is_drawn() {
    let _turn = counter::exclusive();
    let watched = NodeRef::new();
    let spaced = scroll(watched, false);
    let harness = spaced.harness;
    let border = drawn(&harness, watched);
    assert!(
        border.origin.y.0 < spaced.before - ROW,
        "the row did not move, so nothing below is checked"
    );

    // The pointer: the middle of the row hits the row.
    let element = element_of(watched);
    let window = harness.app().windows().first().expect("a window");
    let middle = Point::new(
        zgui_geom::DevicePx(border.origin.x.0 + border.size.width.0 / 2.0),
        zgui_geom::DevicePx(border.origin.y.0 + border.size.height.0 / 2.0),
    );
    assert_eq!(
        window.chain_at(middle).last(),
        Some(&element),
        "the middle of the row hit something else"
    );

    // The accessibility tree: what a consumer composes is where the row is drawn.
    let tree = support::a11y_tree(&harness);
    let state = tree.state();
    let row = state
        .node_by_tree_local_id(NodeId(element.as_u64()), TreeId::ROOT)
        .expect("the row is in the tree");
    let composed = row.bounding_box().expect("the row has a rectangle");
    assert_eq!(
        (composed.y0, composed.height()),
        (
            f64::from(border.origin.y.0),
            f64::from(border.size.height.0)
        ),
        "{composed:?} against {border:?}"
    );
}

/// A positioned port holding a sticky header, rows, and a box fixed inside it.
const PINNED_CSS: &str = "
root { display: block; width: 400px; height: 300px; background-color: #ffffff }
.port { display: block; position: relative; width: 400px; height: 240px; overflow: scroll;
        background-color: #101010 }
.head { display: block; width: 400px; height: 30px; position: sticky; top: 0px;
        background-color: #303030 }
.row { display: block; width: 360px; height: 20px; background-color: #202020 }
.fab { display: block; position: fixed; top: 180px; left: 300px; width: 40px; height: 30px;
       background-color: #ff0000 }
";

/// The three elements of the pinned page a test reads back.
#[derive(Clone, Copy)]
struct Pinned {
    /// The scrollport.
    port: NodeRef,
    /// The sticky header.
    head: NodeRef,
    /// The fixed box.
    fab: NodeRef,
}

/// A window holding the pinned page.
fn pinned(refs: Pinned) -> Harness<Runtime> {
    support::app_with_text(PINNED_CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        let mut port = zgui_elements::column()
            .class("port")
            .node_ref(refs.port)
            .child(
                zgui_elements::control()
                    .class("head")
                    .node_ref(refs.head)
                    .a11y(A11yBinding::new(Role::Button).label("Head")),
            );
        for _ in 0..200 {
            port = port.child(zgui_elements::column().class("row"));
        }
        port = port.child(
            zgui_elements::control()
                .class("fab")
                .node_ref(refs.fab)
                .a11y(A11yBinding::new(Role::Button).label("Add")),
        );
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(port)
                .into_view()
                .build(cx),
        )
    })
}

/// Where the pinned page draws its port's top edge, its header and its fixed box after each turn.
type Pins = Vec<(f32, [f32; 4], [f32; 4])>;

/// A rectangle as four numbers that compare.
fn numbers(rect: zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device>) -> [f32; 4] {
    [
        rect.origin.x.0,
        rect.origin.y.0,
        rect.size.width.0,
        rect.size.height.0,
    ]
}

/// Turns the wheel over the pinned page by each of `turns`, and reads back where it draws the
/// port's top edge, the header and the fixed box after each turn, and whether the port draws its
/// content in a space of its own.
fn pinned_turns(turns: &[f32]) -> (Pins, Vec<bool>, Harness<Runtime>, Pinned) {
    let refs = Pinned {
        port: NodeRef::new(),
        head: NodeRef::new(),
        fab: NodeRef::new(),
    };
    let mut harness = pinned(refs);
    harness.settle(8);
    let mut pins = Vec::new();
    let mut held = Vec::new();
    for &lines in turns {
        wheel(&mut harness, lines);
        let window = harness.app().windows().first().expect("a window");
        let layout = window.layout().borrow();
        let port = *layout
            .fragments_of(element_of(refs.port))
            .first()
            .expect("the port has a piece");
        let port = layout.fragment(port).expect("a live piece");
        held.push(
            port.flags
                .contains(zgui_layout::FragmentFlags::SCROLLS_CONTENT),
        );
        let top = zgui_layout::fragment::transform::placed::onto_device(
            port.padding_box,
            port.transform,
            &window.placements(),
        )
        .origin
        .y
        .0;
        drop(layout);
        pins.push((
            top,
            numbers(drawn(&harness, refs.head)),
            numbers(drawn(&harness, refs.fab)),
        ));
    }
    (pins, held, harness, refs)
}

#[test]
fn a_sticky_header_and_a_fixed_box_in_a_content_space_are_drawn_hit_and_reported_where_they_stand()
{
    let _turn = counter::exclusive();
    let turns = [1.0, 3.0, 2.0, 6.0, -4.0];
    let (spaced, held, harness, refs) = pinned_turns(&turns);
    assert!(
        held.iter().all(|held| *held),
        "the port gave its content space up over a sticky and a fixed box: {held:?}"
    );
    let (port_top, first_head, fab) = spaced[0];
    for (step, (top, head, at)) in spaced.iter().enumerate() {
        assert_eq!(*top, port_top, "after turn {step}, the port moved");
        assert_eq!(
            head[1], *top,
            "after turn {step}, the header is not at the top edge of its port"
        );
        assert_eq!(
            [head[0], head[2], head[3]],
            [first_head[0], first_head[2], first_head[3]],
            "after turn {step}, the header changed its x, width or height"
        );
        assert_eq!(*at, fab, "after turn {step}, the fixed box moved");
    }

    // The pointer finds both where they are drawn, and so does the accessibility tree for the
    // header. A fixed box inside a scrolled port is reported relative to the scrolled content
    // with and without content spaces, so its node is not compared here.
    let window = harness.app().windows().first().expect("a window");
    let tree = support::a11y_tree(&harness);
    let state = tree.state();
    for (watched, rect) in [(refs.head, spaced[4].1), (refs.fab, fab)] {
        let element = element_of(watched);
        let middle = Point::new(
            zgui_geom::DevicePx(rect[0] + rect[2] / 2.0),
            zgui_geom::DevicePx(rect[1] + rect[3] / 2.0),
        );
        assert_eq!(
            window.chain_at(middle).last(),
            Some(&element),
            "the middle of {rect:?} hit something else"
        );
        if watched == refs.fab {
            continue;
        }
        let node = state
            .node_by_tree_local_id(NodeId(element.as_u64()), TreeId::ROOT)
            .expect("the element is in the tree");
        let composed = node.bounding_box().expect("the element has a rectangle");
        assert_eq!(
            (composed.y0, composed.height()),
            (f64::from(rect[1]), f64::from(rect[3])),
            "{composed:?} against {rect:?}"
        );
    }
}
