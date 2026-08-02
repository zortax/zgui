//! What the glyph cache must and must not ask the rasteriser for.

use core::sync::atomic::{AtomicUsize, Ordering};

use zgui_atlas::{Atlas, AtlasLimits};
use zgui_geom::{DevicePx, Point, Size};
use zgui_text::{
    FaceId, GlyphFormat, GlyphImage, GlyphKey, GlyphRaster, RasterStyle, SubpixelOffset,
};

use super::GlyphCache;

/// A rasteriser that counts what it was asked for and answers from a script.
struct Counting {
    /// How many times [`GlyphRaster::raster`] was called.
    calls: AtomicUsize,
    /// The extent every glyph rasterises to; zero for a glyph with no pixels.
    extent: Size<u32, zgui_geom::Device>,
}

impl Counting {
    /// A rasteriser producing one glyph of `extent`.
    fn new(extent: Size<u32, zgui_geom::Device>) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            extent,
        }
    }
}

impl GlyphRaster for Counting {
    fn raster(&self, _key: &GlyphKey) -> Option<GlyphImage> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let pixels = self.extent.width as usize * self.extent.height as usize;
        Some(GlyphImage {
            size: self.extent,
            placement: Point::new(DevicePx(1.0), DevicePx(7.0)),
            format: GlyphFormat::Mono,
            bytes: vec![255; pixels],
        })
    }

    /// No curves: every case here is about what the atlas remembers.
    fn outline(&self, _key: &zgui_text::OutlineKey) -> Option<zgui_text::GlyphOutline> {
        None
    }
}

/// The key every case here asks for.
fn key() -> GlyphKey {
    GlyphKey::new(
        FaceId(1),
        42,
        16.0,
        SubpixelOffset(0),
        RasterStyle::Grayscale,
    )
}

#[test]
fn a_glyph_already_in_the_atlas_is_not_rasterised_again() {
    // The defect this exists for: the tile was in the atlas and the placement was not, so the only
    // way to learn where the pixels went was to make them again. A second frame drawing the same
    // word must cost no rasterisation at all.
    let mut atlas = Atlas::new(AtlasLimits::default());
    let raster = Counting::new(Size::new(5, 9));
    let mut cache = GlyphCache::default();

    let first = cache.tile_for(&mut atlas, &raster, &key());
    let second = cache.tile_for(&mut atlas, &raster, &key());

    assert_eq!(
        raster.calls.load(Ordering::Relaxed),
        1,
        "the second lookup asked nobody"
    );
    let first = first.expect("the first placed the glyph");
    let second = second.expect("the second found it");
    assert_eq!(first.tile, second.tile);
    assert_eq!(first.key, second.key);
    assert_eq!(
        (first.placement, first.size),
        (second.placement, second.size),
        "a cached answer places the pixels exactly where making them again would have"
    );
}

#[test]
fn a_glyph_with_no_pixels_is_not_rasterised_again_either() {
    // A space rasterises to nothing, so nothing is inserted into the atlas for it and an atlas
    // lookup can never answer for it. Every space on the page ran the face's hinting program again
    // on every full repaint until absence became a cached answer in its own right.
    let mut atlas = Atlas::new(AtlasLimits::default());
    let raster = Counting::new(Size::new(0, 0));
    let mut cache = GlyphCache::default();

    assert!(cache.tile_for(&mut atlas, &raster, &key()).is_none());
    assert!(cache.tile_for(&mut atlas, &raster, &key()).is_none());
    assert!(cache.tile_for(&mut atlas, &raster, &key()).is_none());

    assert_eq!(
        raster.calls.load(Ordering::Relaxed),
        1,
        "the emptiness was remembered"
    );
    assert_eq!(cache.len(), 1);
}

#[test]
fn an_evicted_tile_is_made_again_rather_than_reported_as_missing() {
    // The remembered atlas key outlives the tile it names. Serving it without checking would draw
    // whatever texels took its place, and treating it as an absence would silently delete the
    // glyph; the entry has to fall back to rasterising.
    let mut atlas = Atlas::new(AtlasLimits::default());
    let raster = Counting::new(Size::new(5, 9));
    let mut cache = GlyphCache::default();

    atlas.begin_frame();
    assert!(cache.tile_for(&mut atlas, &raster, &key()).is_some());
    atlas.begin_frame();
    let freed = atlas.evict_least_recently_used();
    assert_eq!(freed.tiles, 1, "the tile this case is about actually went");

    assert!(
        cache.tile_for(&mut atlas, &raster, &key()).is_some(),
        "the glyph still draws"
    );
    assert_eq!(
        raster.calls.load(Ordering::Relaxed),
        2,
        "and it was made again to do so"
    );
}

#[test]
fn clearing_forgets_every_key() {
    // A lost device empties the atlas. An entry kept here would name a tile that no longer exists,
    // and the next allocation to take that key would be drawn with this glyph's placement.
    let mut atlas = Atlas::new(AtlasLimits::default());
    let raster = Counting::new(Size::new(5, 9));
    let mut cache = GlyphCache::default();

    assert!(cache.tile_for(&mut atlas, &raster, &key()).is_some());
    cache.clear();
    assert_eq!(cache.len(), 0);
    assert!(cache.tile_for(&mut atlas, &raster, &key()).is_some());
    assert_eq!(raster.calls.load(Ordering::Relaxed), 2);
}
