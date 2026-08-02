//! Decoded images reaching the display list through the content cache.
//!
//! The glyph half of the cache is exercised where a text engine is available; this is the other
//! half, and it is the one a document draws a picture through. What it has to show is that a
//! replaced fragment resolves to a tile of this framework's own atlas, that the tile is the extent
//! of the image rather than of the box, and that a replaced box with nothing attached draws nothing
//! at all rather than a placeholder.

mod support;

use zgui_atlas::{AtlasLimits, MemorySink};
use zgui_geom::Size;
use zgui_paint::ContentCache;
use zgui_text::NoRaster;

use support::{Element, Harness};

/// A document with one replaced box, sized by its style.
const CSS: &str = "root { display: block; width: 200px; height: 100px }
                   picture { display: block; width: 64px; height: 32px }";

/// The fixture tree: a root with one replaced child.
fn tree() -> Element {
    Element::new("root").children(vec![Element::new("picture").image()])
}

/// Sixteen by eight of opaque red, premultiplied, four bytes a texel.
fn image() -> (Size<u32, zgui_geom::Device>, Vec<u8>) {
    let size = Size::new(16u32, 8u32);
    let texels = (0..size.width * size.height)
        .flat_map(|_| [255u8, 0, 0, 255])
        .collect();
    (size, texels)
}

/// An image attached to a replaced node is drawn as a colour sprite over its content box.
#[test]
fn an_attached_image_becomes_a_colour_sprite_over_the_content_box() {
    let mut harness = Harness::new(tree(), CSS);
    harness.compose(200.0, 100.0);

    let mut cache = ContentCache::new(AtlasLimits::default());
    let mut sink = MemorySink::new();
    let (size, texels) = image();
    cache
        .set_image(harness.replaced_id("picture"), size, texels)
        .expect("the buffer matches the extent");

    harness.paint_content(&mut cache, &NoRaster);

    let sprites = &harness.scene().primitives.color_sprites;
    assert_eq!(
        sprites.len(),
        1,
        "one replaced box with content attached is one colour sprite"
    );
    let sprite = &sprites[0];
    let content_box = harness
        .store()
        .fragment(harness.fragment_of("picture"))
        .expect("the replaced box produced a fragment")
        .content_box;
    assert_eq!(
        sprite.bounds,
        [
            content_box.origin.x.0,
            content_box.origin.y.0,
            content_box.size.width.0,
            content_box.size.height.0,
        ],
        "the sprite covers the content box, which is what an image is stretched to"
    );
    assert_eq!(
        [sprite.tile.bounds[2], sprite.tile.bounds[3]],
        [16, 8],
        "the tile is the image's own extent, not the box's"
    );

    cache.flush(&mut sink).expect("the in-memory sink accepts");
    assert_eq!(
        sink.bytes_written(),
        16 * 8 * 4,
        "the texels have to reach the texture before anything samples it"
    );
}

/// A replaced box with nothing attached draws nothing, and says so by drawing nothing.
#[test]
fn a_replaced_box_with_no_content_draws_no_sprite() {
    let mut harness = Harness::new(tree(), CSS);
    harness.compose(200.0, 100.0);

    let mut cache = ContentCache::new(AtlasLimits::default());
    harness.paint_content(&mut cache, &NoRaster);

    assert!(
        harness.scene().primitives.color_sprites.is_empty(),
        "an image that has not been decoded yet is not a grey rectangle"
    );
    assert_eq!(cache.report().tiles, 0);
}

/// The same image drawn twice is one tile.
#[test]
fn one_image_drawn_twice_is_one_tile() {
    let mut harness = Harness::new(tree(), CSS);
    harness.compose(200.0, 100.0);

    let mut cache = ContentCache::new(AtlasLimits::default());
    let id = harness.replaced_id("picture");
    let (size, texels) = image();
    cache.set_image(id, size, texels).expect("well formed");

    harness.paint_content(&mut cache, &NoRaster);
    harness.paint_content(&mut cache, &NoRaster);
    assert_eq!(
        cache.report().tiles,
        1,
        "drawing an image twice must not upload it twice"
    );

    // Detaching the content leaves the tile where it is: the atlas frees space by eviction, and a
    // tile freed the moment its last user let go would be re-uploaded by the next frame that drew
    // the same picture again.
    cache.remove_image(id);
    assert_eq!(cache.report().tiles, 1);
}
