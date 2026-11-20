//! Decoded images reaching the compositor through the content cache.
//!
//! The glyph half of the cache is exercised where a text engine is available; this is the other
//! half, and it is the one a document draws a picture through. What it has to show is that a
//! replaced fragment draws one compositor image of the picture's own texels, that the image is the
//! extent of the picture rather than of the box, and that a replaced box with nothing attached
//! draws nothing at all rather than a placeholder.

mod support;

use zgui_atlas::AtlasLimits;
use zgui_geom::Size;
use zgui_paint::ContentCache;
use zgui_testkit_compositor::{Drawing, Instance};

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

/// The images the whole output draws.
fn images(drawing: &Drawing) -> Vec<&Instance> {
    drawing.of_kind("color").collect()
}

/// An image attached to a replaced node is drawn as one image over its content box.
#[test]
fn an_attached_image_becomes_an_image_over_the_content_box() {
    let mut harness = Harness::new(tree(), CSS);
    harness.compose(200.0, 100.0);

    let mut cache = ContentCache::new(AtlasLimits::default());
    let (size, texels) = image();
    cache
        .set_image(harness.replaced_id("picture"), size, texels)
        .expect("the buffer matches the extent");

    harness.paint_content(&mut cache);
    assert_eq!(
        harness.compositor.commit.images_written, 1,
        "the texels reach the compositor before anything samples them"
    );

    let drawing = harness.redraw();
    let [image] = images(&drawing)[..] else {
        panic!("one replaced box with content attached is one image: {drawing}");
    };
    let content_box = harness
        .store()
        .fragment(harness.fragment_of("picture"))
        .expect("the replaced box produced a fragment")
        .content_box();
    assert_eq!(
        image.bounds(),
        [
            content_box.origin.x.0,
            content_box.origin.y.0,
            content_box.size.width.0,
            content_box.size.height.0,
        ],
        "the image covers the content box, which is what a picture is stretched to"
    );
    let tile = image.field("tile").expect("a tile");
    assert!(
        tile.ends_with(", 16, 8]"),
        "the image is the picture's own extent, not the box's: {drawing}"
    );
}

/// A replaced box with nothing attached draws nothing, and says so by drawing nothing.
#[test]
fn a_replaced_box_with_no_content_draws_no_image() {
    let mut harness = Harness::new(tree(), CSS);
    harness.compose(200.0, 100.0);

    let mut cache = ContentCache::new(AtlasLimits::default());
    harness.paint_content(&mut cache);

    assert!(
        images(&harness.redraw()).is_empty(),
        "an image that has not been decoded yet is not a grey rectangle"
    );
    assert_eq!(harness.compositor.commit.images_written, 0);
}

/// The same image drawn twice is written once.
#[test]
fn one_image_drawn_twice_is_written_once() {
    let mut harness = Harness::new(tree(), CSS);
    harness.compose(200.0, 100.0);

    let mut cache = ContentCache::new(AtlasLimits::default());
    let id = harness.replaced_id("picture");
    let (size, texels) = image();
    cache.set_image(id, size, texels).expect("well formed");

    harness.paint_content(&mut cache);
    assert_eq!(harness.compositor.commit.images_written, 1);
    harness.paint_content(&mut cache);
    assert_eq!(
        harness.compositor.commit.images_written, 0,
        "drawing an image twice must not write it twice"
    );
}

/// Detaching a picture lets the compositor's image go once no chunk draws it.
#[test]
fn a_detached_picture_releases_its_image() {
    let mut harness = Harness::new(tree(), CSS);
    harness.compose(200.0, 100.0);

    let mut cache = ContentCache::new(AtlasLimits::default());
    let id = harness.replaced_id("picture");
    let (size, texels) = image();
    cache.set_image(id, size, texels).expect("well formed");
    harness.paint_content(&mut cache);
    assert_eq!(images(&harness.redraw()).len(), 1);

    cache.remove_image(id);
    harness.paint_content(&mut cache);
    assert!(images(&harness.redraw()).is_empty());
    assert!(
        harness.compositor.sync.released >= 1,
        "the image no chunk draws is removed: {:?}",
        harness.compositor.sync
    );
}

/// Once the frame is committed, a shared attachment's texels are given back and the compositor's
/// image serves it. An image the compositor no longer holds is reported for a re-decode rather than
/// drawn wrong or lost quietly.
#[test]
fn a_settled_attachment_is_served_by_its_image_and_reported_when_the_image_goes() {
    let mut harness = Harness::new(tree(), CSS);
    harness.compose(200.0, 100.0);

    let mut cache = ContentCache::new(AtlasLimits::default());
    let id = harness.replaced_id("picture");
    let (size, texels) = image();
    cache
        .set_image_shared(id, 7, size, std::sync::Arc::new(texels))
        .expect("well formed");

    harness.paint_content(&mut cache);
    assert!(
        cache.image_bytes() > 0,
        "the texels are held until the frame settles"
    );
    assert!(cache.image_tile_resident(7));

    cache.settle_uploaded(0);
    assert_eq!(
        cache.image_bytes(),
        0,
        "a shared attachment the compositor holds keeps no host bytes"
    );

    harness.paint_content(&mut cache);
    assert_eq!(
        images(&harness.redraw()).len(),
        1,
        "the compositor's image alone serves the picture"
    );
    assert!(
        cache.take_missing_images().is_empty(),
        "nothing is missing while the compositor holds the image"
    );

    // A fresh painter and compositor hold no image, which is what a rejected commit leaves.
    let mut fresh = Harness::new(tree(), CSS);
    fresh.compose(200.0, 100.0);
    let id = fresh.replaced_id("picture");
    cache.set_image_uploaded(id, 7, size);

    fresh.paint_content(&mut cache);
    assert!(
        images(&fresh.redraw()).is_empty(),
        "a picture the compositor holds no image of draws nothing this frame"
    );
    assert_eq!(
        cache.take_missing_images(),
        vec![id],
        "and is reported exactly once for a re-decode"
    );
    assert!(!cache.image_tile_resident(7));
}

/// Two nodes attached under one shared handle draw one image of one shared buffer.
#[test]
fn two_nodes_sharing_a_handle_share_one_image() {
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

    harness.paint_content(&mut cache);
    assert_eq!(
        harness.compositor.commit.images_written, 1,
        "one decode shown twice is one image, which is what the shared handle buys"
    );

    let drawing = harness.redraw();
    let shown = images(&drawing);
    assert_eq!(shown.len(), 2, "each node draws the image: {drawing}");
    assert_eq!(shown[0].field("tile"), shown[1].field("tile"), "{drawing}");
}
