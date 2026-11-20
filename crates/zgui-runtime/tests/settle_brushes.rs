//! What colour text is drawn in when it was first shaped inside a mid-frame settle.
//!
//! A virtualised list builds its rows when a geometry observation is delivered, which happens in
//! the middle of the frame: the delivery is flushed, and the document is restyled and laid out
//! again before the frame paints. Text shaped there claims its brush slot against the cascade
//! result it was styled by, exactly as text shaped in the main pass does — and the main pass runs
//! the brush step between its cascade and its layout, so a slot claimed there is one the frame
//! knows about. A settle that skips the step leaves the row shaped into a slot nothing rewrites,
//! and every later colour change misses it: the cascade reports the new colour and the glyphs
//! keep the old one, for as long as the shaping survives.

mod support;

use zgui_interned::{ClassName, CustomPropertyName};
use zgui_platform::SurfaceEvent;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::*;
use zgui_view::{AnyView, BuildCx, IntoView, NodeRef, View};

/// Three rows of text, coloured through a custom property declared on the root.
const CSS: &str = ":root { display: block; width: 400px; height: 300px }
                   .rows { display: block }
                   .row { display: block; height: 20px }
                   text { display: block }
                   .c { color: var(--accent) }";

/// The grey every glyph in the window is drawn in, ordered down the window and then across it.
fn drawn_greys(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> Vec<u8> {
    let drawing = support::drawing(&harness.app().windows()[0]);
    let mut placed: Vec<_> = drawing
        .glyphs()
        .iter()
        .map(|glyph| {
            let [x, y, ..] = support::world(glyph);
            let [red, ..] = glyph
                .color("paint")
                .expect("a glyph is drawn in a solid colour");
            (y.to_bits(), x.to_bits(), (red * 255.0).round() as u8)
        })
        .collect();
    placed.sort_unstable();
    placed.into_iter().map(|(_, _, grey)| grey).collect()
}

/// Damages the whole window without moving anything in it, so every glyph is emitted again
/// through the slot it named when it was shaped.
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

/// The rows, mounted only once the root's geometry has been observed — which is the moment a
/// virtualised list learns how many rows it has room for, and it is inside the frame.
fn rows_born_in_a_settle(cx: &mut BuildCx<'_>, grey: RwSignal<u8>) -> Box<dyn zgui_view::Anchor> {
    let handle = NodeRef::new();
    let born = RwSignal::new(false);
    core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
        if handle.get().is_none() {
            return;
        }
        let box_of = handle.observe_border_box();
        core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
            if box_of.get().is_some() {
                born.set(true);
            }
        }));
    }));
    let rows = zgui_view::Show::new(
        move || born.get(),
        || {
            let mut rows = zgui_elements::column().class("rows");
            for _ in 0..3 {
                rows = rows.child(
                    zgui_elements::column().class("row").child(
                        zgui_elements::text()
                            .class_toggle(ClassName::new("c"), true)
                            .child("aa"),
                    ),
                );
            }
            AnyView::new(rows)
        },
    )
    .fallback(|| AnyView::new(()))
    .into_view();
    let view = zgui_elements::column()
        .node_ref(handle)
        .custom_property(CustomPropertyName::new("accent"), move || {
            let level = grey.get();
            Some(format!("rgb({level}, {level}, {level})"))
        })
        .child(rows)
        .into_view();
    Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
}

#[test]
fn text_shaped_inside_a_settle_follows_a_later_colour_change() {
    let grey = RwSignal::new(30_u8);
    let mut harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        rows_born_in_a_settle(cx, grey)
    });
    harness.settle(8);
    repaint_all(&mut harness);

    // The control: the rows were born, and born in the first colour.
    assert_eq!(
        drawn_greys(&harness),
        [30; 6],
        "the rows were not mounted by the observation, so nothing here is being tested"
    );

    grey.set(210);
    harness.settle(8);
    repaint_all(&mut harness);
    assert_eq!(
        drawn_greys(&harness),
        [210; 6],
        "text shaped inside the settle kept the colour it was shaped in"
    );

    grey.set(60);
    harness.settle(8);
    repaint_all(&mut harness);
    assert_eq!(drawn_greys(&harness), [60; 6]);
    harness.shut_down();
}
