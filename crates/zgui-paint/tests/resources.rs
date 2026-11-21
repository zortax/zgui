//! What the emit walk is allowed to touch.
//!
//! Building a frame decides *what* is drawn, and the compositor decides where a picture's texels
//! go and when a device hears of them. This is the assertion that the two stay apart: a walk that
//! reaches no renderer and hands its pictures to the compositor through the frame's transaction.

mod support;

use zgui_geom::{Device, Size};
use zgui_layout::tree::store::LayoutStore;
use zgui_paint::{ContentCache, FrameContent};
use zgui_text::ShapedGlyphs;

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
    ) -> FrameContent<'a> = ContentCache::frame;

    // And the walk itself, which takes what it reads and the scene it writes and nothing else.
    let _emit: fn(
        &mut zgui_paint::Painter,
        &zgui_paint::PaintInput<'_>,
        &mut zgui_paint::Tables,
    ) -> zgui_paint::PaintReport = zgui_paint::Painter::emit;

    // A whole frame, drawn through a cache that holds a picture, with no sink anywhere in the call.
    let mut harness = Harness::new(tree(), CSS);
    harness.compose(200.0, 100.0);
    let mut cache = ContentCache::new();
    let (size, texels) = image();
    cache
        .set_image(harness.replaced_id("picture"), size, texels)
        .expect("the buffer matches the extent");
    harness.paint_content(&mut cache);

    assert_eq!(
        harness.compositor.commit.images_written, 1,
        "the frame really did hand a picture on, so the absence of a device is not vacuous"
    );
}
