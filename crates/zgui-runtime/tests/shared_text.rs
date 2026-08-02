//! Two strings that are byte-for-byte the same, in the same font, in two different colours.
//!
//! A shaped paragraph is cached under a key derived from everything a shaping pass reads, and a
//! colour is deliberately not part of a shaping pass — re-theming an element must not force its
//! text to be shaped again. What the shaped result *carries*, though, is the brush slot each run
//! is drawn through, so a key that leaves the brush out lets two paragraphs that differ only in
//! colour share one entry — and both are then drawn through whichever paragraph was shaped first.
//!
//! That is a tooltip's shape exactly: a short, common string in the application's ordinary font,
//! in a colour of its own because it sits on an inverted surface. Whether it is drawn in its own
//! colour then depends on whether some other element with the same string shaped before it — and
//! a change of device scale re-shapes every string in the window under fresh keys, re-deciding the
//! race the other way, which is a window whose text changes colour when it is dragged onto another
//! monitor with nothing restyled at all.

mod support;

use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, View};

/// Two elements holding the same two characters, one dark and one light.
const CSS: &str = ":root { display: block; width: 400px; height: 300px }
                   text { display: block }
                   .one { display: block; color: rgb(10, 10, 10) }
                   .two { display: block; color: rgb(200, 200, 200) }";

/// The window's surface, which is what a platform event is addressed to.
fn surface_of(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) -> zgui_platform::SurfaceId {
    harness
        .platform()
        .offscreens()
        .first()
        .map(|surface| zgui_platform::Surface::id(surface.as_ref()))
        .expect("the application opened its window")
}

/// The grey every glyph is drawn in, ordered down the window.
fn drawn_greys(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> Vec<u8> {
    let mut placed: Vec<_> = harness.app().windows()[0]
        .scene()
        .primitives
        .mono_sprites
        .iter()
        .map(|sprite| {
            (
                sprite.bounds[1].to_bits(),
                sprite.bounds[0].to_bits(),
                (sprite.color[0] * 255.0).round() as u8,
            )
        })
        .collect();
    placed.sort_unstable();
    placed.into_iter().map(|(_, _, grey)| grey).collect()
}

#[test]
fn two_equal_strings_in_two_colours_are_drawn_in_two_colours() {
    let mut harness = support::app_with_text(CSS, |cx: &mut BuildCx<'_>| {
        let view = zgui_elements::column()
            .child(
                zgui_elements::column()
                    .class("one")
                    .child(zgui_elements::text().child("aa")),
            )
            .child(
                zgui_elements::column()
                    .class("two")
                    .child(zgui_elements::text().child("aa")),
            )
            .into_view();
        Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(8);

    assert_eq!(
        drawn_greys(&harness),
        [10, 10, 200, 200],
        "two strings that differ only in colour were drawn in one"
    );

    // The same window on a monitor at another scale. Every paragraph is shaped again under fresh
    // keys, so which of the two strings shapes first is decided again — a window that passes above
    // by luck of ordering fails here, which is the "it changes when I drag it across" report.
    let surface = surface_of(&harness);
    harness.deliver(
        surface,
        SurfaceEvent::ScaleFactorChanged {
            scale_factor: 1.2,
            size: zgui_geom::Size::new(zgui_geom::DevicePx(400.0), zgui_geom::DevicePx(300.0)),
        },
    );
    harness.advance(std::time::Duration::from_millis(50));
    harness.settle(16);

    assert_eq!(
        drawn_greys(&harness),
        [10, 10, 200, 200],
        "the window moved to another monitor and one string took the other's colour"
    );
}
