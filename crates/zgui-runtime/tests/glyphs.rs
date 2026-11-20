//! Text reaching the compositor: the glyphs, where they land, and in which colour.
//!
//! Every other test in this crate runs a window with no text engine at all, which is exactly how a
//! pipeline that shaped and laid out a paragraph and then emitted nothing for it stayed green. The
//! assertions here are therefore about **glyph sprites specifically**, never about an instance
//! count, and about where each one landed relative to the line box the fragment tree reports.
//!
//! The fixed face makes that possible without a font file: a cluster advances half the font size
//! and the ascent is four fifths of it. The compositor's test rasteriser draws every glyph as a box
//! on the baseline from the pen, half the font size wide and three quarters of it high. At the
//! initial size that is eight wide and twelve tall.

mod support;

use zgui_geom::DevicePx;
use zgui_view::{BuildCx, IntoView, View};

/// The sheet every fixture here is styled by.
///
/// The padding exists so that a line box at the origin and a line box positioned by layout are
/// different rectangles: an emitter that dropped the line's origin and drew every glyph from the
/// surface corner would still be green against an unpadded root.
const CSS: &str = "root { display: block; width: 400px; height: 300px; padding: 12px 20px }
                   text { display: block }";

/// One cluster's advance at the initial font size, in device pixels.
const ADVANCE: f32 = 8.0;
/// The extent of one glyph box at that size.
const GLYPH: (f32, f32) = (8.0, 12.0);
/// The content area of the fixed face at that size: four fifths above the baseline, one below.
const ASCENT: f32 = 12.8;
/// The whole of it, which is what any leading is measured against.
const CONTENT_AREA: f32 = 16.0;

/// Where the baseline of a line box sits, measured from its own top edge.
///
/// A line box is at least as tall as its content area and is usually taller, because `line-height`
/// adds leading — which is split evenly above and below. Deriving it from the line box the fragment
/// tree reports rather than hard-coding it is what keeps the assertion about the *glyph* rather than
/// about the user-agent sheet's line height.
fn baseline_of(line: zgui_geom::Rect<DevicePx, zgui_geom::Device>) -> f32 {
    let leading = (line.size.height.0 - CONTENT_AREA) / 2.0;
    (leading + ASCENT).round()
}

/// A window holding one paragraph of `words`.
fn paragraph(words: &'static str) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::text().child(words))
                .into_view()
                .build(cx),
        )
    })
}

/// The glyph sprites `window` shows, in window space and in draw order.
fn glyph_boxes(window: &zgui_runtime::Window) -> Vec<[f32; 4]> {
    support::drawing(window)
        .glyphs()
        .into_iter()
        .map(support::world)
        .collect()
}

/// A paragraph of real text becomes glyph sprites, at the positions layout put its line box.
#[test]
fn mounting_text_puts_glyphs_in_the_display_list_where_layout_said() {
    let mut app = paragraph("abc");
    app.settle(8);

    let window = &app.app().windows()[0];
    let line = support::first_line_box(window);
    let drawing = support::drawing(window);
    let sprites = glyph_boxes(window);

    assert_eq!(
        sprites.len(),
        3,
        "three characters shaped and laid out must be three glyph sprites, not a background quad \
         and a hope: {drawing}"
    );
    // The control on the control: a line box at the origin would make every assertion below hold
    // for an emitter that ignored the placement entirely.
    assert!(
        line.origin.x.0 > 0.0 && line.origin.y.0 > 0.0,
        "the padding must have moved the line box off the surface corner, and did not: {line:?}"
    );

    for (index, [x, y, width, height]) in sprites.into_iter().enumerate() {
        assert_eq!(
            x,
            line.origin.x.0 + index as f32 * ADVANCE,
            "glyph {index} is not at the pen position its own advances put it at: {drawing}"
        );
        assert_eq!(
            y + height,
            line.origin.y.0 + baseline_of(line),
            "a glyph box sits directly on the baseline, and this one does not: {line:?} in \
             {drawing}"
        );
        assert_eq!(
            (width, height),
            GLYPH,
            "the sprite's extent is the glyph box's, not the line box's: {drawing}"
        );
        assert!(
            y >= line.origin.y.0 && y + height <= line.bottom().0,
            "glyph {index} left its own line box {line:?}: {drawing}"
        );
    }
}

/// A space advances the pen and draws nothing, which is what a coverage mask of a space is.
#[test]
fn a_space_moves_the_pen_and_emits_no_sprite() {
    let mut app = paragraph("a b");
    app.settle(8);

    let window = &app.app().windows()[0];
    let line = support::first_line_box(window);
    let sprites = glyph_boxes(window);

    assert_eq!(
        sprites.len(),
        2,
        "two letters and a space is two glyphs: {}",
        support::drawing(window)
    );
    assert_eq!(
        sprites[1][0],
        line.origin.x.0 + 2.0 * ADVANCE,
        "the space still advanced the pen, so the second letter is two clusters along"
    );
}

/// The glyphs are drawn from the brush table, in the colour the cascade put in it.
///
/// This is the outcome of the brush step, and the assertion is written so that it can only pass for
/// that reason. A run whose slot resolves to nothing falls back to the *element's* own colour — so
/// the run is put inside an inline element of a different colour from the block that contains it.
/// The block's colour is what the fallback would produce; the inline element's is what the table
/// holds. They are different colours, so only one of the two can be on the screen.
#[test]
fn glyphs_are_drawn_through_the_brush_table_and_not_the_fallback() {
    let sheet = "root { display: block; width: 400px; height: 300px }
                 text { display: block; color: rgb(255, 0, 0) }
                 label { color: rgb(0, 0, 255) }";
    let mut app = support::app_with_text(sheet, |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::text().child(zgui_elements::label().child("abc")))
                .into_view()
                .build(cx),
        )
    });
    app.settle(8);

    let (brushes, _) = app.app_mut().windows_mut()[0].text_brushes_held();
    assert!(
        brushes > 0,
        "the text engine's brush table is what the emitter reads, and nothing ever filled it"
    );
    let window = &app.app().windows()[0];
    let drawing = support::drawing(window);
    let glyphs = drawing.glyphs();
    assert_eq!(glyphs.len(), 3, "the label drew no glyphs: {drawing}");
    for glyph in glyphs {
        assert_eq!(
            glyph.color("paint"),
            Some([0.0, 0.0, 1.0, 1.0]),
            "the glyphs were drawn in the containing block's colour, which is what a run whose \
             brush slot resolved to nothing falls back to: {drawing}"
        );
    }
}

/// The glyphs a window draws after moving to a new device pixel ratio are recorded at the new
/// device size.
#[test]
fn a_ratio_change_records_the_glyphs_at_the_new_device_size() {
    /// The extent on screen of the first glyph sprite.
    fn first_extent(app: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> (f32, f32) {
        let window = &app.app().windows()[0];
        let [_, _, width, height] = *glyph_boxes(window)
            .first()
            .expect("the paragraph produced a glyph sprite");
        (width, height)
    }

    let mut app = paragraph("abc");
    app.settle(8);
    let at_one = first_extent(&app);
    assert_eq!(
        at_one, GLYPH,
        "the fixture is not drawing the glyph it is documented to draw"
    );

    app.deliver_to_first(zgui_platform::SurfaceEvent::ScaleFactorChanged {
        scale_factor: 2.0,
        size: zgui_geom::Size::new(DevicePx(800.0), DevicePx(600.0)),
    });
    app.settle(8);
    let at_two = first_extent(&app);

    assert_eq!(
        at_two,
        (at_one.0 * 2.0, at_one.1 * 2.0),
        "the glyph does not occupy twice the device pixels at twice the ratio: {}",
        support::drawing(&app.app().windows()[0])
    );

    app.shut_down();
}
