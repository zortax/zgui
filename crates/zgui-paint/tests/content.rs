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

/// A pool with no room evicts a cold generation and retries, rather than drawing nothing.
#[test]
fn a_full_pool_makes_room_for_a_new_picture_by_evicting_a_cold_one() {
    let mut harness = Harness::new(tree(), CSS);
    harness.compose(200.0, 100.0);

    // One texture of 64 texels square per pool: a 48-square tile fills it past reuse, so a second
    // 48-square picture has nowhere to go until the first is evicted.
    let limits = AtlasLimits {
        texture_size: 64,
        max_texture_size: 64,
        max_textures_per_pool: 1,
        soft_bytes: None,
    };
    let mut cache = ContentCache::new(limits);
    let id = harness.replaced_id("picture");
    let size = Size::new(48u32, 48u32);
    let texels = || {
        std::sync::Arc::new(
            (0..size.width * size.height)
                .flat_map(|_| [255u8, 0, 0, 255])
                .collect::<Vec<u8>>(),
        )
    };

    cache
        .set_image_shared(id, 1, size, texels())
        .expect("well formed");
    harness.paint_content(&mut cache, &NoRaster);
    assert_eq!(cache.report().tiles, 1, "the first picture fills the pool");

    cache
        .set_image_shared(id, 2, size, texels())
        .expect("well formed");
    harness.paint_content(&mut cache, &NoRaster);

    assert_eq!(
        harness.scene().primitives.color_sprites.len(),
        1,
        "the new picture is drawn: a full pool is a reason to evict, never to draw nothing"
    );
    assert_eq!(
        cache.report().tiles,
        1,
        "the cold picture went and the new one has its room"
    );
}

/// After a flush, a shared attachment's texels are given back and the tile serves it; a tile
/// that later goes missing is reported for a re-decode rather than drawn wrong or lost quietly.
#[test]
fn a_settled_attachment_is_served_by_its_tile_and_reported_when_the_tile_goes() {
    let mut harness = Harness::new(tree(), CSS);
    harness.compose(200.0, 100.0);

    let mut cache = ContentCache::new(AtlasLimits::default());
    let id = harness.replaced_id("picture");
    let (size, texels) = image();
    cache
        .set_image_shared(id, 7, size, std::sync::Arc::new(texels))
        .expect("well formed");

    harness.paint_content(&mut cache, &NoRaster);
    let mut sink = MemorySink::new();
    cache.flush(&mut sink).expect("the in-memory sink accepts");
    assert!(
        cache.image_bytes() > 0,
        "the texels are held until the flush settles"
    );

    cache.settle_uploaded(0);
    assert_eq!(
        cache.image_bytes(),
        0,
        "a flushed shared attachment holds no host bytes"
    );

    harness.paint_content(&mut cache, &NoRaster);
    assert_eq!(
        harness.scene().primitives.color_sprites.len(),
        1,
        "the tile alone serves the picture"
    );
    assert!(
        cache.take_missing_images().is_empty(),
        "nothing is missing while the tile is resident"
    );

    // Ordinary eviction cannot take a shown tile — the replay records hold it — so the way a
    // tile actually disappears from under an uploaded attachment is a lost device. A fresh
    // walk with no records is what re-emits and notices.
    cache.clear();
    let mut fresh = Harness::new(tree(), CSS);
    fresh.compose(200.0, 100.0);
    let id = fresh.replaced_id("picture");
    cache.set_image_uploaded(id, 7, size);

    fresh.paint_content(&mut cache, &NoRaster);
    assert!(
        fresh.scene().primitives.color_sprites.is_empty(),
        "a picture whose tile is gone draws nothing this frame"
    );
    assert_eq!(
        cache.take_missing_images(),
        vec![id],
        "and is reported exactly once for a re-decode"
    );
}

/// Content attached with levels of detail gets a texture of exactly its own size.
#[test]
fn a_mipped_attachment_gets_a_standalone_texture_at_its_own_extent() {
    let mut harness = Harness::new(tree(), CSS);
    harness.compose(200.0, 100.0);

    let mut cache = ContentCache::new(AtlasLimits::default());
    let size = Size::new(600u32, 600u32);
    let texels = std::sync::Arc::new(vec![9u8; (size.width * size.height * 4) as usize]);
    let mips = vec![zgui_paint::MipLevel {
        size: Size::new(300, 300),
        texels: std::sync::Arc::new(vec![9u8; 300 * 300 * 4]),
    }];
    cache
        .set_image_shared_mipped(harness.replaced_id("picture"), 7, size, size, texels, mips)
        .expect("well formed");

    harness.paint_content(&mut cache, &NoRaster);
    let mut sink = MemorySink::new();
    cache.flush(&mut sink).expect("the in-memory sink accepts");

    use zgui_atlas::{TextureId, TextureKind};
    assert_eq!(
        sink.size_of(TextureId::new(TextureKind::Image, 0)),
        Some(Size::new(600, 600)),
        "the texture is the image, exactly, rather than a page grown to hold it"
    );
}

/// Two nodes attached under one shared handle resolve to one tile of one shared buffer.
#[test]
fn two_nodes_sharing_a_handle_share_one_tile() {
    let tree = Element::new("root").children(vec![
        Element::new("picture").image(),
        Element::new("picture").image(),
    ]);
    let mut harness = Harness::new(tree, CSS);
    harness.compose(200.0, 100.0);

    let mut cache = ContentCache::new(AtlasLimits::default());
    let (size, texels) = image();
    let texels = std::sync::Arc::new(texels);
    let ids = harness.replaced_ids("picture");
    assert_eq!(ids.len(), 2, "the fixture has two pictures");
    for id in &ids {
        cache
            .set_image_shared(*id, 7, size, std::sync::Arc::clone(&texels))
            .expect("well formed");
    }

    harness.paint_content(&mut cache, &NoRaster);

    assert_eq!(
        harness.scene().primitives.color_sprites.len(),
        2,
        "each node draws its own sprite"
    );
    assert_eq!(
        cache.report().tiles,
        1,
        "one decode shown twice is one tile, which is what the shared handle buys"
    );
    let mut sink = MemorySink::new();
    cache.flush(&mut sink).expect("the in-memory sink accepts");
    assert_eq!(
        sink.bytes_written(),
        16 * 8 * 4,
        "and one upload: the second node added no bytes"
    );
}
