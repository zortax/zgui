//! What a nested inline box paints with the line it is on.
//!
//! The fixed face advances eight pixels a character, so every rectangle below is arithmetic: the
//! chip after `aa ` starts 24 pixels in, its padding is 4 pixels either side and its four
//! characters are 32 pixels wide.

mod support;

use zgui_interned::ClassName;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::*;
use zgui_testkit_compositor::Drawing;
use zgui_view::{BuildCx, IntoView, View};

const CSS: &str = ":root { display: block; width: 400px; height: 300px }
                   .para { display: block }
                   .chip { padding: 0 4px; background-color: rgb(255, 0, 0) }
                   .chip.on { background-color: rgb(0, 0, 255) }
                   .under { text-decoration: underline rgb(0, 255, 0) }
                   .shadowed { text-shadow: 2px 2px rgb(255, 0, 255) }
                   .plain { text-shadow: none }
                   .sup { vertical-align: super }
                   .lit.on { background-color: rgb(0, 0, 255) }";

/// A solid quad the window shows: its place in the draw order, its colour as 8-bit sRGB and its
/// rectangle.
fn quads(drawing: &Drawing) -> Vec<(usize, [u8; 3], [f32; 4])> {
    drawing
        .instances()
        .iter()
        .enumerate()
        .filter(|(_, instance)| instance.kind() == "quad")
        .filter_map(|(order, quad)| {
            let [red, green, blue, alpha] = quad.color("fill")?;
            let rgb = [red, green, blue].map(|channel| (channel / alpha * 255.0).round() as u8);
            Some((order, rgb, support::world(quad)))
        })
        .collect()
}

/// Every glyph the window shows: its place in the draw order, its premultiplied colour and its
/// rectangle.
fn glyphs(drawing: &Drawing) -> Vec<(usize, Option<[f32; 4]>, [f32; 4])> {
    drawing
        .instances()
        .iter()
        .enumerate()
        .filter(|(_, instance)| matches!(instance.kind(), "mono" | "subpixel" | "color"))
        .map(|(order, glyph)| (order, glyph.color("paint"), support::world(glyph)))
        .collect()
}

/// What the window of `harness` shows.
fn shown(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> Drawing {
    support::drawing(&harness.app().windows()[0])
}

/// A paragraph of `aa ` and a chip holding `bbbb`, with `chip` deciding the chip's classes.
fn chip_view(
    on: RwSignal<bool>,
    chip_class: &'static str,
) -> impl FnMut(&mut BuildCx<'_>) -> Box<dyn zgui_view::Anchor> {
    move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::r#box()
                .class("para")
                .child(zgui_elements::text().child(|| "aa "))
                .child(
                    zgui_elements::text()
                        .class(chip_class)
                        .class_toggle(ClassName::new("on"), move || on.get())
                        .child(|| "bbbb"),
                )
                .into_view()
                .build(cx),
        ) as Box<dyn zgui_view::Anchor>
    }
}

#[test]
fn a_chip_is_painted_under_its_glyphs_across_its_own_range() {
    let on = RwSignal::new(false);
    let mut harness = support::app_with_text(CSS, chip_view(on, "chip"));
    harness.settle(8);
    let window = &harness.app().windows()[0];
    let line = support::first_line_box(window);
    let drawing = support::drawing(window);

    let chips: Vec<_> = quads(&drawing)
        .into_iter()
        .filter(|(_, rgb, _)| *rgb == [255, 0, 0])
        .collect();
    assert_eq!(chips.len(), 1, "one chip, one quad: {chips:?}");
    let (order, _, bounds) = chips[0];
    assert_eq!(
        bounds[0] - line.origin.x.0,
        24.0,
        "the chip starts after `aa `"
    );
    assert_eq!(
        bounds[2],
        4.0 + 32.0 + 4.0,
        "the padding, then four characters"
    );

    let glyphs: Vec<_> = glyphs(&drawing)
        .into_iter()
        .map(|(order, _, bounds)| (order, bounds))
        .collect();
    assert!(!glyphs.is_empty(), "the line drew no glyphs");
    let over = |glyph: &[f32; 4]| {
        glyph[0] < bounds[0] + bounds[2]
            && bounds[0] < glyph[0] + glyph[2]
            && glyph[1] < bounds[1] + bounds[3]
            && bounds[1] < glyph[1] + glyph[3]
    };
    assert!(
        glyphs
            .iter()
            .filter(|(_, glyph)| over(glyph))
            .all(|(glyph, _)| *glyph > order),
        "the chip is painted over its glyphs: chip at {order}, glyphs {glyphs:?}"
    );
    assert!(
        glyphs
            .iter()
            .any(|(_, glyph)| glyph[0] - line.origin.x.0 == 24.0 + 4.0),
        "the chip's first glyph sits inside its padding: {glyphs:?}"
    );
    harness.shut_down();
}

#[test]
fn a_chip_that_changes_colour_is_painted_in_the_new_one() {
    let on = RwSignal::new(false);
    let mut harness = support::app_with_text(CSS, chip_view(on, "chip"));
    harness.settle(8);

    for step in 0..6 {
        on.set(step % 2 == 0);
        harness.settle(8);
        let want = if step % 2 == 0 {
            [0, 0, 255]
        } else {
            [255, 0, 0]
        };
        let colours: Vec<_> = quads(&shown(&harness))
            .into_iter()
            .map(|(_, rgb, _)| rgb)
            .filter(|rgb| *rgb == [255, 0, 0] || *rgb == [0, 0, 255])
            .collect();
        assert_eq!(
            colours,
            [want],
            "step {step}: the line replayed the chip's old colour"
        );
    }
    harness.shut_down();
}

#[test]
fn a_decorated_chip_draws_its_line_across_its_own_text() {
    let on = RwSignal::new(false);
    let mut harness = support::app_with_text(CSS, chip_view(on, "chip under"));
    harness.settle(8);
    let window = &harness.app().windows()[0];
    let line = support::first_line_box(window);
    let drawing = support::drawing(window);

    let lines: Vec<_> = drawing
        .of_kind("line")
        .map(|line| (line.color("color"), support::world(line)))
        .collect();
    assert_eq!(lines.len(), 1, "one underlined chip, one line: {lines:?}");
    let (color, bounds) = lines[0];
    assert_eq!(
        color,
        Some([0.0, 1.0, 0.0, 1.0]),
        "in the chip's decoration colour"
    );
    assert_eq!(
        bounds[0] - line.origin.x.0,
        24.0 + 4.0,
        "from the start of the chip's text"
    );
    assert_eq!(bounds[2], 32.0, "across its four characters");
    let glyph_bottom = glyphs(&drawing)
        .iter()
        .map(|(_, _, glyph)| glyph[1] + glyph[3])
        .fold(0.0_f32, f32::max);
    let centre = bounds[1] + bounds[3] * 0.5;
    assert!(
        centre > glyph_bottom && centre < line.origin.y.0 + line.size.height.0,
        "under the baseline the glyphs sit on and inside the line: centre {centre}, glyphs end \
         at {glyph_bottom}, line {line:?}"
    );
    harness.shut_down();
}

/// The x of every glyph drawn in a colour, relative to the line box.
fn glyphs_in(
    drawing: &Drawing,
    line: zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device>,
    color: [f32; 4],
) -> Vec<f32> {
    let mut xs: Vec<f32> = glyphs(drawing)
        .into_iter()
        .filter(|(_, paint, _)| *paint == Some(color))
        .map(|(_, _, bounds)| bounds[0] - line.origin.x.0)
        .collect();
    xs.sort_by(f32::total_cmp);
    xs
}

#[test]
fn a_text_shadow_follows_only_the_run_that_declared_it() {
    let on = RwSignal::new(false);
    let mut harness = support::app_with_text(CSS, chip_view(on, "chip shadowed"));
    harness.settle(8);
    let window = &harness.app().windows()[0];
    let line = support::first_line_box(window);
    let shadows = glyphs_in(&support::drawing(window), line, [1.0, 0.0, 1.0, 1.0]);
    // The chip's four glyphs start after `aa ` and its padding, moved two pixels.
    assert_eq!(
        shadows,
        [30.0, 38.0, 46.0, 54.0],
        "a shadow under each of the chip's glyphs and under nothing else"
    );
    harness.shut_down();
}

#[test]
fn a_run_that_declares_no_text_shadow_drops_the_inherited_one() {
    let on = RwSignal::new(false);
    let mut harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::r#box()
                .class("para shadowed")
                .child(zgui_elements::text().child(|| "aa "))
                .child(
                    zgui_elements::text()
                        .class("chip plain")
                        .class_toggle(ClassName::new("on"), move || on.get())
                        .child(|| "bbbb"),
                )
                .into_view()
                .build(cx),
        ) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(8);
    let window = &harness.app().windows()[0];
    let line = support::first_line_box(window);
    let shadows = glyphs_in(&support::drawing(window), line, [1.0, 0.0, 1.0, 1.0]);
    assert_eq!(
        shadows,
        [2.0, 10.0],
        "the paragraph's two letters are shadowed and the chip's are not"
    );
    harness.shut_down();
}

#[test]
fn a_superscript_draws_its_glyphs_raised_by_its_shift() {
    let on = RwSignal::new(false);
    let mut harness = support::app_with_text(CSS, chip_view(on, "sup"));
    harness.settle(8);
    let window = &harness.app().windows()[0];
    let line = support::first_line_box(window);
    let tops: Vec<(f32, f32)> = glyphs(&support::drawing(window))
        .into_iter()
        .map(|(_, _, glyph)| (glyph[0] - line.origin.x.0, glyph[1]))
        .collect();
    let plain = tops
        .iter()
        .find(|(x, _)| *x == 0.0)
        .expect("the first letter")
        .1;
    let raised: Vec<f32> = tops
        .iter()
        .filter(|(x, _)| *x >= 24.0)
        .map(|(_, top)| *top)
        .collect();
    assert_eq!(raised.len(), 4, "{tops:?}");
    // A third of the 16 px font, in whole pixels.
    let shift = (16.0 * zgui_layout::inline::vertical_align::SUPER_FRACTION).round();
    assert!(
        raised.iter().all(|top| *top == plain - shift),
        "the superscript's glyphs sit {shift} px above the paragraph's: {tops:?}"
    );
    harness.shut_down();
}

#[test]
fn a_run_that_starts_painting_is_painted_and_one_that_stops_is_cleared() {
    // The line holding a plain run paints no part of it, so the first background is a change of
    // what the line paints and not only of a colour.
    let on = RwSignal::new(false);
    let mut harness = support::app_with_text(CSS, chip_view(on, "lit"));
    harness.settle(8);
    let blue = |harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>| {
        quads(&shown(harness))
            .into_iter()
            .filter(|(_, rgb, _)| *rgb == [0, 0, 255])
            .count()
    };
    assert_eq!(blue(&harness), 0);
    on.set(true);
    harness.settle(8);
    assert_eq!(blue(&harness), 1, "the run's new background is painted");
    on.set(false);
    harness.settle(8);
    assert_eq!(blue(&harness), 0, "and cleared again");
    harness.shut_down();
}
