//! The seam a custom element's own shaped runs reach the atlas through.
//!
//! [`GlyphPlacementSource`] is the one way a run a caller shaped itself becomes tiles, and it has
//! to behave exactly like the paragraph path where it matters: the same cache, the same phase
//! split, and every tile it hands out named to the frame so that a fragment which replays keeps
//! them.

use std::sync::Mutex;

use zgui_atlas::AtlasLimits;
use zgui_dom::Document;
use zgui_geom::{DevicePx, Point, Size};
use zgui_layout::tree::store::LayoutStore;
use zgui_paint::walk::replay::hold::ResourceOwner;
use zgui_paint::{ContentCache, GlyphPlacementSource, PlacedGlyph};
use zgui_scene::PaintSlot;
use zgui_text::{
    FaceId, GlyphFormat, GlyphImage, GlyphKey, GlyphRaster, RasterStyle, ShapedGlyph, ShapedRun,
};

/// A rasteriser answering uniformly, recording every key it was asked for.
#[derive(Default)]
struct Recording {
    /// Every key, in order.
    asked: Mutex<Vec<GlyphKey>>,
}

impl GlyphRaster for Recording {
    fn raster(&self, key: &GlyphKey) -> Option<GlyphImage> {
        self.asked.lock().expect("no panic").push(*key);
        Some(GlyphImage {
            size: Size::new(5, 9),
            placement: Point::new(DevicePx(1.0), DevicePx(7.0)),
            format: GlyphFormat::Mono,
            bytes: vec![255; 45],
        })
    }

    fn outline(&self, _key: &zgui_text::OutlineKey) -> Option<zgui_text::GlyphOutline> {
        None
    }
}

/// A shaper that answers nothing: these cases never go through a paragraph.
struct NoParagraphs;

impl zgui_text::ShapedGlyphs for NoParagraphs {
    fn visit_line(
        &self,
        _paragraph: zgui_text::ParagraphKey,
        _line: u16,
        _visit: &mut dyn FnMut(ShapedRun<'_>),
    ) {
    }
}

/// Three glyphs a whole pixel apart, so each is its own cache entry.
const GLYPHS: [ShapedGlyph; 3] = [
    ShapedGlyph {
        glyph: 1,
        x: 0.0,
        y: 12.0,
    },
    ShapedGlyph {
        glyph: 2,
        x: 8.0,
        y: 12.0,
    },
    ShapedGlyph {
        glyph: 3,
        x: 16.0,
        y: 12.0,
    },
];

/// A run over those glyphs.
fn run() -> ShapedRun<'static> {
    ShapedRun {
        face: FaceId(1),
        size: 16.0,
        synthetic_bold: 0.0,
        synthetic_slant: 0.0,
        has_color: false,
        brush: PaintSlot(0),
        glyphs: &GLYPHS,
    }
}

#[test]
fn a_callers_own_run_becomes_tiles_through_the_frames_atlas() {
    let mut content = ContentCache::new(AtlasLimits::default());
    let document = Document::new();
    let store = LayoutStore::new(document.store().document());
    let raster = Recording::default();

    let mut placed = Vec::new();
    {
        let frame = content.frame(&store, &NoParagraphs, &raster);
        frame.place_run(
            &run(),
            RasterStyle::Grayscale,
            Point::new(DevicePx(10.0), DevicePx(20.0)),
            &mut placed,
        );
    }

    assert_eq!(placed.len(), 3, "one placement per glyph");
    assert_eq!(content.report().tiles, 3, "and one tile each");
    assert_eq!(raster.asked.lock().expect("no panic").len(), 3);
    for glyph in &placed {
        assert_eq!(glyph.bounds.size, Size::new(DevicePx(5.0), DevicePx(9.0)));
    }
}

#[test]
fn the_same_run_at_the_same_phase_asks_the_rasteriser_once() {
    let mut content = ContentCache::new(AtlasLimits::default());
    let document = Document::new();
    let store = LayoutStore::new(document.store().document());
    let raster = Recording::default();
    let origin = Point::new(DevicePx(10.0), DevicePx(20.0));

    let mut first = Vec::new();
    let mut second = Vec::new();
    {
        let frame = content.frame(&store, &NoParagraphs, &raster);
        frame.place_run(&run(), RasterStyle::Grayscale, origin, &mut first);
    }
    {
        let frame = content.frame(&store, &NoParagraphs, &raster);
        frame.place_run(&run(), RasterStyle::Grayscale, origin, &mut second);
    }

    assert_eq!(first, second, "the same answer, placed identically");
    assert_eq!(
        raster.asked.lock().expect("no panic").len(),
        3,
        "the second frame drew from the cache"
    );
    assert_eq!(content.report().tiles, 3);
}

#[test]
fn a_fractional_origin_moves_the_phase_rather_than_the_pixel() {
    let mut content = ContentCache::new(AtlasLimits::default());
    let document = Document::new();
    let store = LayoutStore::new(document.store().document());
    let raster = Recording::default();

    let mut whole = Vec::new();
    let mut fractional = Vec::new();
    {
        let frame = content.frame(&store, &NoParagraphs, &raster);
        frame.place_run(
            &run(),
            RasterStyle::Grayscale,
            Point::new(DevicePx(10.0), DevicePx(20.0)),
            &mut whole,
        );
        frame.place_run(
            &run(),
            RasterStyle::Grayscale,
            Point::new(DevicePx(10.5), DevicePx(20.0)),
            &mut fractional,
        );
    }

    let asked = raster.asked.lock().expect("no panic");
    assert_eq!(asked.len(), 6, "a different phase is a different entry");
    assert_ne!(
        asked[0].offset, asked[3].offset,
        "half a pixel moved the phase"
    );
    // The tiles are whole-pixel rectangles in both cases.
    for glyph in whole.iter().chain(fractional.iter()) {
        assert_eq!(glyph.bounds.origin.x.0.fract(), 0.0);
        assert_eq!(glyph.bounds.origin.y.0.fract(), 0.0);
    }
}

#[test]
fn every_tile_placed_is_named_to_the_frame() {
    let mut content = ContentCache::new(AtlasLimits::default());
    let document = Document::new();
    let store = LayoutStore::new(document.store().document());
    let raster = Recording::default();

    let mut named = Vec::new();
    let mut placed: Vec<PlacedGlyph> = Vec::new();
    {
        let frame = content.frame(&store, &NoParagraphs, &raster);
        frame.place_run(
            &run(),
            RasterStyle::Grayscale,
            Point::new(DevicePx(10.0), DevicePx(20.0)),
            &mut placed,
        );
        frame.take_named(&mut named);
    }

    assert_eq!(
        named.len(),
        placed.len(),
        "a fragment that replays these primitives retains exactly the tiles they read"
    );
}
