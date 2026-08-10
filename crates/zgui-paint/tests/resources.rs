//! What the emit walk is allowed to touch, and what it is allowed to leave unfinished.
//!
//! Building a frame decides *what* is drawn. Where a rasterised glyph or picture ended up in a
//! texture is a separate answer, owned by a cache this window keeps, and performing anything about
//! it on a device is a third thing again, owned by whoever has one. These are the assertions that
//! the three stay apart: a walk that reaches no renderer, a walk that makes no device call, and a
//! frame that cannot hand a device a sprite whose content was never placed.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_atlas::{
    AtlasLimits, MemorySink, SinkError, TextureFormat, TextureId, TextureKind, TextureSink,
};
use zgui_geom::{Device, Rect, Size};
use zgui_layout::tree::store::LayoutStore;
use zgui_paint::{ContentCache, FrameContent};
use zgui_text::{GlyphRaster, NoRaster, ShapedGlyphs};

use support::{Element, Harness};

/// A document with one replaced box, sized by its style.
const CSS: &str = "root { display: block; width: 200px; height: 100px }
                   picture { display: block; width: 64px; height: 32px }";

/// The fixture tree: a root with one replaced child.
fn tree() -> Element {
    Element::new("root").children(vec![Element::new("picture").image()])
}

/// Sixteen by eight of opaque red, premultiplied, four bytes a texel.
fn image() -> (Size<u32, Device>, Vec<u8>) {
    let size = Size::new(16u32, 8u32);
    let texels = (0..size.width * size.height)
        .flat_map(|_| [255u8, 0, 0, 255])
        .collect();
    (size, texels)
}

/// How many times a device was asked to do anything.
#[derive(Clone, Debug, Default)]
struct Calls {
    /// Textures created.
    created: Rc<Cell<u32>>,
    /// Regions written.
    written: Rc<Cell<u32>>,
    /// Textures destroyed.
    destroyed: Rc<Cell<u32>>,
}

impl Calls {
    /// Every call of every kind.
    fn total(&self) -> u32 {
        self.created.get() + self.written.get() + self.destroyed.get()
    }
}

/// A sink that keeps no texels and remembers only that it was spoken to.
///
/// Deliberately not [`MemorySink`]: what is under test is whether the device is reached at all, and
/// a sink that stored bytes would invite an assertion about the bytes instead.
struct Counting {
    /// What it has been asked to do.
    calls: Calls,
}

impl TextureSink for Counting {
    fn create_texture(
        &mut self,
        _texture: TextureId,
        _size: Size<i32, Device>,
        _format: TextureFormat,
    ) -> Result<(), SinkError> {
        self.calls.created.set(self.calls.created.get() + 1);
        Ok(())
    }

    fn write_texture(
        &mut self,
        _texture: TextureId,
        _bounds: Rect<i32, Device>,
        _format: TextureFormat,
        _bytes: &[u8],
    ) -> Result<(), SinkError> {
        self.calls.written.set(self.calls.written.get() + 1);
        Ok(())
    }

    fn destroy_texture(&mut self, _texture: TextureId) {
        self.calls.destroyed.set(self.calls.destroyed.get() + 1);
    }
}

/// Nothing the emit walk reaches names a renderer or a texture sink.
///
/// Written as the signature itself rather than as a behaviour, because that is the whole claim: the
/// borrow a walk used to need is not absent by convention, it is absent from the type, and a caller
/// that tried to supply one would not compile.
#[test]
fn emit_borrows_no_renderer() {
    let _borrow: for<'a> fn(
        &'a mut ContentCache,
        &'a LayoutStore,
        &'a dyn ShapedGlyphs,
        &'a dyn GlyphRaster,
    ) -> FrameContent<'a> = ContentCache::frame;

    // And the walk itself, which takes what it reads and the scene it writes and nothing else.
    let _emit: fn(
        &mut zgui_paint::Painter,
        &zgui_paint::PaintInput<'_>,
        &mut zgui_scene::Scene,
    ) -> zgui_paint::PaintReport = zgui_paint::Painter::emit;

    // A whole frame, drawn through a cache that has to allocate a tile and grow a pool to hold it,
    // with no sink anywhere in the call.
    let mut harness = Harness::new(tree(), CSS);
    harness.compose(200.0, 100.0);
    let mut cache = ContentCache::new(AtlasLimits::default());
    let (size, texels) = image();
    cache
        .set_image(harness.replaced_id("picture"), size, texels)
        .expect("the buffer matches the extent");
    harness.paint_content(&mut cache, &NoRaster);

    assert_eq!(
        cache.report().tiles,
        1,
        "the frame really did cache something, so the absence of a device is not vacuous"
    );
}

/// Between the start of a frame and the end of its walk, no device is asked to do anything.
#[test]
fn no_device_call_during_emit() {
    let mut harness = Harness::new(tree(), CSS);
    harness.compose(200.0, 100.0);

    let mut cache = ContentCache::new(AtlasLimits::default());
    let (size, texels) = image();
    cache
        .set_image(harness.replaced_id("picture"), size, texels)
        .expect("the buffer matches the extent");

    let calls = Calls::default();
    let mut sink = Counting {
        calls: calls.clone(),
    };

    harness.paint_content(&mut cache, &NoRaster);
    assert_eq!(
        calls.total(),
        0,
        "a walk that rasterised a picture into a fresh atlas still spoke to no device"
    );

    cache.flush(&mut sink).expect("the sink accepts everything");
    assert_eq!(
        calls.created.get(),
        1,
        "the texture the tile went into is created at the flush, and it is created exactly once"
    );
    assert_eq!(
        calls.written.get(),
        1,
        "and its texels leave with it, so the assertion above is about timing rather than absence"
    );
}

/// A tile is placed by arithmetic, and only what the device must hear about is deferred.
#[test]
fn a_tile_is_placed_before_anything_has_a_device() {
    let mut cache = ContentCache::new(AtlasLimits::default());
    let mut harness = Harness::new(tree(), CSS);
    harness.compose(200.0, 100.0);
    let (size, texels) = image();
    cache
        .set_image(harness.replaced_id("picture"), size, texels)
        .expect("the buffer matches the extent");
    harness.paint_content(&mut cache, &NoRaster);

    let sprite = harness.scene().primitives.color_sprites[0];
    assert!(
        !sprite.tile.is_unresolved(),
        "rasterising as the walk reaches content places it as the walk reaches it"
    );
    assert_eq!(
        [sprite.tile.bounds[2], sprite.tile.bounds[3]],
        [16, 8],
        "and the placement is the image's own extent, decided with no device in reach"
    );

    let mut sink = MemorySink::new();
    cache.flush(&mut sink).expect("the sink accepts everything");
    assert_eq!(
        sink.size_of(TextureId::new(TextureKind::Image, 0))
            .map(|extent| extent.width > 0),
        Some(true),
        "the texture the tile was placed in exists once something has a device"
    );
}
