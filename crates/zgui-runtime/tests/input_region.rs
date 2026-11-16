//! A window whose input region follows the boxes of some of its elements.
//!
//! A pop-over covers more of the screen than its card. Only the card may take presses, and it has
//! to stay that way as the window moves to a display of another density.

mod support;

use zgui_geom::{Css, CssPx, DevicePx, Point, Rect, Size};
use zgui_platform::SurfaceEvent;
use zgui_runtime::windows::{use_input_region, use_window};
use zgui_view::{BuildCx, IntoView, NodeRef, View};

/// A card of a known size in a known place, on a root that fills the window.
const CSS: &str = "root { display: block; width: 100%; height: 100% }
                   .card { display: block; width: 120px; height: 40px; margin: 30px 0 0 50px }";

/// A rectangle in CSS pixels.
fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<CssPx, Css> {
    Rect::new(
        Point::new(CssPx(x), CssPx(y)),
        Size::new(CssPx(width), CssPx(height)),
    )
}

/// The last input region the window gave its surface.
fn last_region(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) -> zgui_platform_headless::Region {
    harness.platform().offscreens()[0]
        .input_region_log()
        .last()
        .cloned()
        .expect("the window gave its surface an input region")
}

#[test]
fn the_input_region_is_the_card_at_every_scale() {
    let mut harness = support::app(CSS, |cx: &mut BuildCx<'_>| {
        let card = NodeRef::new();
        // Never bound, so it is never laid out and adds nothing to the region.
        let absent = NodeRef::new();
        use_input_region([card, absent]);
        let view = zgui_elements::column()
            .class("root")
            .child(zgui_elements::r#box().class("card").node_ref(card))
            .into_view();
        Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(16);
    assert_eq!(
        last_region(&harness),
        Some(vec![rect(50.0, 30.0, 120.0, 40.0)])
    );

    // The card is laid out in device pixels at the new scale, and the region stays in CSS pixels.
    harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
        scale_factor: 1.25,
        size: Size::new(DevicePx(500.0), DevicePx(375.0)),
    });
    harness.settle(16);
    assert_eq!(
        last_region(&harness),
        Some(vec![rect(50.0, 30.0, 120.0, 40.0)])
    );

    let log = harness.platform().offscreens()[0].input_region_log();
    assert!(
        log.windows(2).all(|pair| pair[0] != pair[1]),
        "a region that did not change was sent again: {log:?}"
    );
    harness.shut_down();
}

#[test]
fn a_region_set_while_the_window_builds_reaches_its_surface() {
    let mut harness = support::app(CSS, |cx: &mut BuildCx<'_>| {
        use_window().set_input_region(Some(Vec::new()));
        let view = zgui_elements::column().class("root").into_view();
        Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(16);
    assert_eq!(last_region(&harness), Some(Vec::new()));
    harness.shut_down();
}
