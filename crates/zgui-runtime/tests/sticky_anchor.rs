//! A sticky header drawn through the compositor, held in its port while the port scrolls.
//!
//! The header is composed where the flow puts it, and the compositor shifts it from the sticky
//! geometry zgui records. Every case compares the header the compositor draws with the place the
//! header's own coordinate system resolves to, which is where zgui composed a sticky header before
//! the compositor held it.

mod support;

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_platform_headless::Harness;
use zgui_runtime::Runtime;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, ScrollDelta, ScrollPhase, Timestamp, WheelEvent};

/// A port of two sections, each a sticky header over rows. Only the header is green.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   .port { display: block; width: 300px; height: 200px; margin-top: 30px;
                           overflow: scroll; background-color: #101010 }
                   .section { display: block }
                   .head { display: block; position: sticky; top: 0; height: 20px;
                           background-color: rgb(10, 200, 30) }
                   .row { display: block; height: 30px; background-color: #202020 }";

/// The window: two sections of twelve rows each.
fn window() -> Harness<Runtime> {
    support::app(CSS, |cx: &mut BuildCx<'_>| {
        let mut port = zgui_elements::column().class("port");
        for _ in 0..2 {
            let mut section = zgui_elements::column()
                .class("section")
                .child(zgui_elements::column().class("head"));
            for _ in 0..12 {
                section = section.child(zgui_elements::column().class("row"));
            }
            port = port.child(section);
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

/// Scrolls the port down by `by` CSS pixels and settles.
fn scroll(harness: &mut Harness<Runtime>, by: f32) {
    harness.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(100.0), CssPx(100.0)),
            delta: ScrollDelta::Pixels(zgui_geom::Size::new(CssPx(0.0), CssPx(by))),
            phase: ScrollPhase::Discrete,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    for _ in 0..40 {
        harness.advance(std::time::Duration::from_millis(16));
        harness.pump();
    }
    harness.settle(8);
}

/// Whether a header whose top is at `top` shows inside the port, which spans 30 to 230.
fn in_port(top: f32) -> bool {
    top > 30.0 - 20.0 && top < 230.0
}

/// The tops of the green quads the compositor draws inside the port, in window space, top first.
fn drawn_heads(harness: &Harness<Runtime>) -> Vec<f32> {
    let drawing = support::drawing(&harness.app().windows()[0]);
    let mut tops: Vec<f32> = drawing
        .quads()
        .iter()
        .filter(|quad| {
            quad.color("fill")
                .is_some_and(|fill| (fill[1] * 255.0).round() as u8 == 200)
        })
        .map(|quad| support::world(quad)[1])
        .filter(|top| in_port(*top))
        .collect();
    tops.sort_by(f32::total_cmp);
    tops
}

/// The tops of the headers' border boxes inside the port, where their coordinate systems place
/// them, top first.
fn placed_heads(harness: &Harness<Runtime>) -> Vec<f32> {
    let window = &harness.app().windows()[0];
    let layout = window.layout().borrow();
    let mut tops: Vec<f32> = layout
        .keys()
        .iter()
        .filter(|key| {
            layout
                .fragments_of_box(**key)
                .first()
                .and_then(|frag| layout.fragment(*frag))
                .is_some_and(|fragment| {
                    fragment
                        .flags
                        .contains(zgui_layout::FragmentFlags::IS_STICKY)
                })
        })
        .filter_map(|key| layout.fragment(*layout.fragments_of_box(*key).first()?))
        .map(|fragment| {
            fragment
                .on_device(fragment.border_box, &window.placements())
                .origin
                .y
                .0
        })
        .filter(|top| in_port(*top))
        .collect();
    tops.sort_by(f32::total_cmp);
    tops
}

#[test]
fn a_sticky_header_is_drawn_where_its_coordinate_system_puts_it_through_a_scroll() {
    let mut harness = window();
    harness.settle(8);
    assert_eq!(
        drawn_heads(&harness),
        placed_heads(&harness),
        "before any scroll"
    );

    // Into the first section: its header sticks to the top of the port.
    scroll(&mut harness, 100.0);
    let drawn = drawn_heads(&harness);
    assert_eq!(
        drawn,
        placed_heads(&harness),
        "a scroll into the first section"
    );
    assert_eq!(
        drawn.first().copied(),
        Some(30.0),
        "the first header is not held at the top of the port: {drawn:?}"
    );

    // Past the end of the first section: its header leaves with it, and the second one sticks.
    scroll(&mut harness, 300.0);
    let drawn = drawn_heads(&harness);
    assert_eq!(
        drawn,
        placed_heads(&harness),
        "a scroll into the second section"
    );
    assert!(
        drawn.iter().any(|top| (*top - 30.0).abs() < 0.01),
        "no header is held at the top of the port: {drawn:?}"
    );
}
