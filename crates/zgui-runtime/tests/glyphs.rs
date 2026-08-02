//! Text reaching the display list: the glyphs, where they land, and what they cost.
//!
//! Every other test in this crate runs a window with no text engine at all, which is exactly how a
//! pipeline that shaped and laid out a paragraph and then emitted nothing for it stayed green. The
//! assertions here are therefore about **glyph primitives specifically** — never about a primitive
//! count — and about where each one landed relative to the line box the fragment tree reports.
//!
//! The fixed face makes that possible without a font file: a cluster advances half the font size,
//! the ascent is four fifths of it, and every glyph rasterises to a filled rectangle of exactly
//! those dimensions sitting on the baseline. At the initial size that is eight wide, thirteen tall
//! after rounding, and the top edge of the rectangle is its own height above the baseline — so a
//! glyph's box starts at the top of the ascent and its left edge is at the pen.

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
/// The extent of one rasterised glyph at that size.
const GLYPH: (f32, f32) = (8.0, 13.0);
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

/// A paragraph of real text becomes glyph sprites, at the positions layout put its line box.
#[test]
fn mounting_text_puts_glyphs_in_the_display_list_where_layout_said() {
    let mut app = paragraph("abc");
    app.settle(8);

    let window = &app.app().windows()[0];
    let line = support::first_line_box(window);
    let sprites = &window.scene().primitives.mono_sprites;

    assert_eq!(
        sprites.len(),
        3,
        "three characters shaped and laid out must be three glyph sprites, not a background quad \
         and a hope: {:?}",
        window.scene().primitives.mono_sprites
    );
    // The control on the control: a line box at the origin would make every assertion below hold
    // for an emitter that ignored the placement entirely.
    assert!(
        line.origin.x.0 > 0.0 && line.origin.y.0 > 0.0,
        "the padding must have moved the line box off the surface corner, and did not: {line:?}"
    );

    for (index, sprite) in sprites.iter().enumerate() {
        let ink = sprite.ink();
        assert_eq!(
            ink.origin.x,
            DevicePx(line.origin.x.0 + index as f32 * ADVANCE),
            "glyph {index} is not at the pen position its own advances put it at: {ink:?}"
        );
        assert_eq!(
            ink.bottom(),
            DevicePx(line.origin.y.0 + baseline_of(line)),
            "a glyph of the fixed face sits directly on the baseline, and this one does not: \
             {ink:?} against {line:?}"
        );
        assert_eq!(
            (ink.size.width.0, ink.size.height.0),
            GLYPH,
            "the sprite's extent is the rasterised glyph's, not the line box's"
        );
        assert!(
            ink.origin.y.0 >= line.origin.y.0 && ink.bottom().0 <= line.bottom().0,
            "glyph {index} left its own line box: {ink:?} against {line:?}"
        );
    }
}

/// The glyphs really came from a rasteriser and an atlas, and were uploaded before the draw.
#[test]
fn the_glyphs_drawn_are_tiles_that_were_rasterised_and_flushed() {
    let mut app = paragraph("abc");
    app.settle(8);

    let window = &app.app().windows()[0];
    let report = window.content().report();
    assert_eq!(report.tiles, 3, "three distinct glyphs are three tiles");
    assert_eq!(
        report.pending_uploads, 0,
        "a tile whose texels never reached the device samples whatever was there before, so the \
         frame must not end with an upload still queued"
    );
    assert!(report.bytes > 0, "no atlas texture was ever created");

    for sprite in &window.scene().primitives.mono_sprites {
        assert!(
            sprite.tile.bounds[2] > 0 && sprite.tile.bounds[3] > 0,
            "a sprite reading an empty rectangle of the atlas draws nothing: {:?}",
            sprite.tile
        );
    }
}

/// One glyph at one subpixel offset is rasterised once, however many times it is drawn.
#[test]
fn the_same_glyph_at_the_same_offset_is_one_tile() {
    let mut repeated = paragraph("aaaa");
    repeated.settle(8);
    let one = repeated.app().windows()[0].content().report().tiles;

    let mut distinct = paragraph("abcd");
    distinct.settle(8);
    let four = distinct.app().windows()[0].content().report().tiles;

    assert_eq!(
        one, 1,
        "the same letter four times over is one rasterisation"
    );
    assert_eq!(
        four, 4,
        "four different letters must not have been collapsed into one, or the assertion above \
         holds for a cache that keys on nothing"
    );
    assert_eq!(
        repeated.app().windows()[0]
            .scene()
            .primitives
            .mono_sprites
            .len(),
        4,
        "one tile is still four sprites: caching a raster is not drawing it once"
    );
}

/// A space advances the pen and draws nothing, which is what a coverage mask of a space is.
#[test]
fn a_space_moves_the_pen_and_emits_no_sprite() {
    let mut app = paragraph("a b");
    app.settle(8);

    let window = &app.app().windows()[0];
    let line = support::first_line_box(window);
    let sprites = &window.scene().primitives.mono_sprites;

    assert_eq!(sprites.len(), 2, "two letters and a space is two glyphs");
    assert_eq!(
        sprites[1].ink().origin.x,
        DevicePx(line.origin.x.0 + 2.0 * ADVANCE),
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

    let window = &app.app().windows()[0];
    assert!(
        !window.scene().text_paints.is_empty(),
        "the display list's brush table is what the emitter reads, and nothing ever filled it"
    );
    for sprite in &window.scene().primitives.mono_sprites {
        assert_eq!(
            sprite.color,
            [0.0, 0.0, 1.0, 1.0],
            "the glyphs were drawn in the containing block's colour, which is what a run whose \
             brush slot resolved to nothing falls back to"
        );
    }
}

/// The glyph a window draws after moving to a new device pixel ratio is a new raster, not the old
/// one stretched.
///
/// The difference is invisible in the display list's *geometry*: a sprite twice as large on screen
/// is what both a re-rasterised glyph and an upscaled one look like. What separates them is the
/// rectangle of the atlas the sprite reads from, which is the raster's own extent in texels — so
/// that is what this asserts, alongside the extent on screen. A renderer handed a one-times tile
/// and told to draw it into a two-times box gives the first assertion and fails the second, and
/// that is exactly the blurry text a doubled display shows.
#[test]
fn a_ratio_change_rasterises_the_glyph_again_at_the_new_device_size() {
    /// The tile's extent in atlas texels, and the sprite's extent on screen.
    fn sprite_extents(
        app: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    ) -> ((i32, i32), (f32, f32)) {
        let window = &app.app().windows()[0];
        let sprite = window
            .scene()
            .primitives
            .mono_sprites
            .first()
            .expect("the paragraph produced a glyph sprite");
        let ink = sprite.ink();
        (
            (sprite.tile.bounds[2], sprite.tile.bounds[3]),
            (ink.size.width.0, ink.size.height.0),
        )
    }

    let mut app = paragraph("abc");
    app.settle(8);
    let (tile_at_one, ink_at_one) = sprite_extents(&app);
    assert_eq!(
        (ink_at_one.0, ink_at_one.1),
        GLYPH,
        "the fixture is not drawing the glyph it is documented to draw"
    );

    app.deliver_to_first(zgui_platform::SurfaceEvent::ScaleFactorChanged {
        scale_factor: 2.0,
        size: zgui_geom::Size::new(DevicePx(800.0), DevicePx(600.0)),
    });
    app.settle(8);
    let (tile_at_two, ink_at_two) = sprite_extents(&app);

    assert_eq!(
        (tile_at_two.0, tile_at_two.1),
        (tile_at_one.0 * 2, tile_at_one.1 * 2),
        "the sprite is reading the same {tile_at_one:?} rectangle of the atlas it read at a ratio \
         of one, so the pixels on a doubled display are a doubled one-times raster"
    );
    assert_eq!(
        (ink_at_two.0, ink_at_two.1),
        (ink_at_one.0 * 2.0, ink_at_one.1 * 2.0),
        "the glyph does not occupy twice the device pixels at twice the ratio"
    );

    app.shut_down();
}

/// Every glyph a real document draws lands on a whole device pixel, in both axes.
///
/// A line box does not sit on the pixel grid. Centring puts its left edge on a half pixel whenever
/// the space left over is odd, and a fractional line height puts every line after the first at a
/// fraction of one. A sprite whose rectangle carries either of those fractions is a coverage tile
/// resampled by the sampler that draws it — the glyph is drawn across two columns of pixels at half
/// weight each, which is exactly the smeared, unevenly spaced text this asserts against.
///
/// The two control assertions are what stop it passing vacuously: the fixture is required to have
/// produced a line box that is *not* on the grid, in each axis, before the sprites are looked at.
#[test]
fn glyph_sprites_land_on_whole_pixels_under_a_line_box_that_does_not() {
    let sheet = "root { display: block; width: 401px; height: 300px; padding: 3px 0 0 0 }
                 text { display: block; text-align: center; line-height: 1.3; width: 55px }";
    let mut app = support::app_with_text(sheet, |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::text().child("abc abc abc"))
                .into_view()
                .build(cx),
        )
    });
    app.settle(8);

    let window = &app.app().windows()[0];
    let lines = support::line_boxes(window);
    assert!(
        lines.len() >= 2,
        "the fixture was supposed to wrap and produced {} line(s)",
        lines.len()
    );
    assert!(
        lines.iter().any(|line| line.origin.x.0.fract() != 0.0),
        "no line box landed off the horizontal grid, so this asserts nothing: {lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.origin.y.0.fract() != 0.0),
        "no line box landed off the vertical grid, so this asserts nothing: {lines:?}"
    );

    let sprites = &window.scene().primitives.mono_sprites;
    assert!(!sprites.is_empty(), "the paragraph drew no glyphs at all");
    for sprite in sprites {
        let ink = sprite.ink();
        assert_eq!(
            (ink.origin.x.0.fract(), ink.origin.y.0.fract()),
            (0.0, 0.0),
            "a glyph tile at {:?} is drawn across two pixels of the surface",
            ink.origin
        );
    }

    app.shut_down();
}
