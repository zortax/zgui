//! Whether a box that was animated arrives at the display list a fresh build would produce.
//!
//! Twenty steps of `opacity` and `transform` on one box, against a window built once at the final
//! values. The two display lists are compared primitive for primitive and group for group, which
//! is what settles whether a difference between the two pictures is decided here or in the
//! renderer.

mod support;

use zgui_platform::SurfaceEvent;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::*;
use zgui_view::{BuildCx, IntoView, View};

const CSS: &str = ":root { display: block; width: 400px; height: 300px }
                   .bar { display: block; height: 20px }
                   .badge { display: block; width: 40px; height: 18px;
                            background-color: rgb(40, 120, 200); color: rgb(255, 255, 255) }
                   text { display: block }";

/// Where the animation ends.
const LAST: f32 = 0.9;

fn badge(cx: &mut BuildCx<'_>, phase: RwSignal<f32>) -> Box<dyn zgui_view::Anchor> {
    let view = zgui_elements::column()
        .child(
            zgui_elements::column().class("bar").child(
                zgui_elements::r#box()
                    .class("badge")
                    .style_property("opacity", move || {
                        Some(format!("{:.3}", 0.4 + 0.6 * phase.get()))
                    })
                    .style_property("transform", move || {
                        Some(format!("translateX({:.2}px)", -20.0 * phase.get()))
                    })
                    .child(zgui_elements::text().child("live")),
            ),
        )
        .into_view();
    Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
}

fn repaint_all(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>) {
    let surface = harness
        .platform()
        .offscreens()
        .first()
        .map(|surface| zgui_platform::Surface::id(surface.as_ref()))
        .expect("the application opened its window");
    harness.deliver(surface, SurfaceEvent::Occluded(true));
    harness.settle(8);
    harness.deliver(surface, SurfaceEvent::Occluded(false));
    harness.settle(8);
    harness.advance(std::time::Duration::from_millis(50));
    harness.settle(8);
}

/// Everything the display list says, in an order that does not depend on how it was produced.
fn transcript(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> Vec<String> {
    let scene = harness.app().windows()[0].scene();
    let mut lines: Vec<String> = Vec::new();
    for quad in &scene.primitives.quads {
        lines.push(format!("quad {quad:?}"));
    }
    for sprite in &scene.primitives.mono_sprites {
        lines.push(format!("sprite {sprite:?}"));
    }
    for group in &scene.primitives.groups {
        lines.push(format!("group {group:?}"));
    }
    lines.sort();
    lines
}

#[test]
fn an_animated_box_reaches_the_display_list_a_fresh_build_produces() {
    let phase = RwSignal::new(0.0_f32);
    let mut animated = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| badge(cx, phase));
    animated.settle(8);
    for step in 1..=20 {
        phase.set(LAST * step as f32 / 20.0);
        animated.settle(8);
    }
    repaint_all(&mut animated);

    let fresh_phase = RwSignal::new(LAST);
    let mut fresh = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| badge(cx, fresh_phase));
    fresh.settle(8);
    repaint_all(&mut fresh);

    let (moved, built) = (transcript(&animated), transcript(&fresh));
    assert!(
        !built.is_empty(),
        "the fresh build drew nothing: {built:#?}"
    );
    assert_eq!(
        moved, built,
        "the animated window's display list differs from a fresh build's"
    );
    animated.shut_down();
    fresh.shut_down();
}
