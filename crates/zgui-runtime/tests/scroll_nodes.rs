//! What a scroll of a long list costs when its port draws the rows in a content space.
//!
//! The claim: after the first scroll, which gives the port its content space, a scroll frame
//! visits the path down to the port and moves no hit entry and no accessibility rectangle of any
//! row. Each count is paired with the same scroll made without content spaces, where it is large.
//! The rows still have to end up where the scroll put them, for the screen, for the pointer and for
//! an assistive technology, so the test reads all three back.
//!
//! Its own test target, because the switch and the counters are process-wide.

mod support;

use zgui_a11y::{NodeId, TreeId};
use zgui_geom::{Css, CssPx, Point};
use zgui_layout::fragment::scroll_nodes;
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

/// Scrolls once to warm up, then measures a scroll of three lines.
fn scroll(nodes: bool, watched: NodeRef) -> Scrolled {
    scroll_nodes::set_enabled(nodes);
    let mut harness = listing(watched, 16);
    harness.settle(8);
    // The first scroll gives the port its content space, which composes and paints every row on
    // the screen once more. The claim is about every scroll after it.
    wheel(&mut harness, 1.0);
    let before = drawn(&harness, watched).origin.y.0;
    let start = counter::snapshot();
    wheel(&mut harness, 3.0);
    let cost = start.delta(&counter::snapshot());
    scroll_nodes::set_enabled(false);
    Scrolled {
        cost,
        before,
        harness,
    }
}

#[test]
fn a_scroll_of_a_long_list_visits_the_path_down_and_moves_no_row_entry() {
    let _turn = counter::exclusive();
    let control = scroll(false, NodeRef::new());
    let watched = NodeRef::new();
    let spaced = scroll(true, watched);

    let after = drawn(&spaced.harness, watched).origin.y.0;
    assert!(
        after < spaced.before - ROW,
        "the row did not move: {} to {after}. A scroll that moved nothing satisfies every count \
         below",
        spaced.before
    );
    if !zgui_profile::COUNTERS_ENABLED {
        return;
    }
    let passes = spaced.cost.layout_passes.max(1);
    assert!(
        control.cost.fragment_boxes_visited > ROWS as u64,
        "the control's fragment pass reached {} boxes, so it did not carry the rows",
        control.cost.fragment_boxes_visited
    );
    assert!(
        control.cost.hit_entries_updated > ROWS as u64,
        "the control updated {} hit entries, so it did not carry the rows",
        control.cost.hit_entries_updated
    );
    assert!(
        spaced.cost.fragment_boxes_visited <= 4 * passes,
        "the fragment pass reached {} boxes over {passes} walks: more than the path down to the \
         port",
        spaced.cost.fragment_boxes_visited
    );
    assert!(
        spaced.cost.hit_entries_updated <= 2 * passes,
        "{} hit entries updated over {passes} walks: more than the port's own bar pieces",
        spaced.cost.hit_entries_updated
    );
    assert!(
        spaced.cost.a11y_remeasured <= 2 * passes,
        "{} accessibility rectangles measured over {passes} walks: more than the port's own",
        spaced.cost.a11y_remeasured
    );
}

#[test]
fn a_row_in_a_content_space_is_hit_and_reported_where_it_is_drawn() {
    let _turn = counter::exclusive();
    let watched = NodeRef::new();
    let spaced = scroll(true, watched);
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
