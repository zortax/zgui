//! The smaller contracts: glyph keys, glyph images, and what a line tells an assistive technology.

use accesskit::{Node, Role, TextDirection};
use zgui_geom::{CssPx, Point, Size};
use zgui_text::{
    ClusterGeometry, FaceId, GlyphFormat, GlyphImage, GlyphKey, RasterStyle, SubpixelOffset,
    TextRunAttributes,
};

/// Two requests for the same glyph at the same size and position share a key, and any difference
/// splits them — which is what makes a cache between a rasteriser and its caller safe.
#[test]
fn a_glyph_key_separates_everything_that_changes_the_pixels() {
    let base = GlyphKey::new(
        FaceId(1),
        42,
        16.0,
        SubpixelOffset(0),
        RasterStyle::Grayscale,
    );
    assert_eq!(
        base,
        GlyphKey::new(
            FaceId(1),
            42,
            16.0,
            SubpixelOffset(0),
            RasterStyle::Grayscale
        ),
    );

    for different in [
        GlyphKey::new(
            FaceId(2),
            42,
            16.0,
            SubpixelOffset(0),
            RasterStyle::Grayscale,
        ),
        GlyphKey::new(
            FaceId(1),
            43,
            16.0,
            SubpixelOffset(0),
            RasterStyle::Grayscale,
        ),
        GlyphKey::new(
            FaceId(1),
            42,
            16.5,
            SubpixelOffset(0),
            RasterStyle::Grayscale,
        ),
        GlyphKey::new(
            FaceId(1),
            42,
            16.0,
            SubpixelOffset(1),
            RasterStyle::Grayscale,
        ),
        GlyphKey::new(
            FaceId(1),
            42,
            16.0,
            SubpixelOffset(0),
            RasterStyle::Subpixel,
        ),
    ] {
        assert_ne!(base, different);
    }
    assert_eq!(base.size(), 16.0);
}

/// Subpixel positions quantise to four per pixel, and a whole-pixel position lands on the first.
#[test]
fn subpixel_offsets_quantise_within_a_pixel() {
    assert_eq!(SubpixelOffset::quantise(0.0), SubpixelOffset(0));
    assert_eq!(SubpixelOffset::quantise(100.0), SubpixelOffset(0));
    assert_eq!(SubpixelOffset::quantise(100.25), SubpixelOffset(1));
    assert_eq!(SubpixelOffset::quantise(100.5), SubpixelOffset(2));
    assert_eq!(SubpixelOffset::quantise(100.75), SubpixelOffset(3));
    // Rounding up past the last step wraps to the next pixel's first, never to a fifth step.
    assert_eq!(SubpixelOffset::quantise(100.99), SubpixelOffset(0));
    assert_eq!(SubpixelOffset(3).to_pixels(), 0.75);
}

/// A glyph image's byte count has to match its extent and its format.
#[test]
fn a_glyph_image_knows_when_its_bytes_do_not_add_up() {
    let mut image = GlyphImage {
        size: Size::new(4, 2),
        placement: Point::new(zgui_geom::DevicePx(0.0), zgui_geom::DevicePx(-8.0)),
        format: GlyphFormat::Mono,
        bytes: vec![0; 8],
    };
    assert!(image.is_well_formed());
    assert!(!image.is_empty());

    image.format = GlyphFormat::Subpixel;
    assert!(!image.is_well_formed(), "three bytes a pixel needs 24");

    image.bytes = vec![0; 24];
    assert!(image.is_well_formed());

    let blank = GlyphImage {
        size: Size::new(0, 0),
        bytes: Vec::new(),
        ..image
    };
    assert!(
        blank.is_empty() && blank.is_well_formed(),
        "a space is both"
    );
}

/// The accessibility arrays are parallel, sum to the text, and reach the node.
#[test]
fn a_text_run_describes_itself_consistently() {
    let text = "héllo";
    let clusters: Vec<ClusterGeometry> = [(0, 1), (1, 2), (3, 1), (4, 1), (5, 1)]
        .iter()
        .scan(0.0f32, |offset, (start, length)| {
            let cluster = ClusterGeometry {
                text: *start..*start + *length,
                offset: CssPx(*offset),
                advance: CssPx(8.0),
            };
            *offset += 8.0;
            Some(cluster)
        })
        .collect();

    let attributes =
        TextRunAttributes::from_clusters(&clusters, vec![0], TextDirection::LeftToRight);

    assert!(
        attributes.is_consistent(text),
        "the arrays agree with the text"
    );
    assert_eq!(attributes.character_lengths, [1, 2, 1, 1, 1]);
    assert_eq!(attributes.character_positions, [0.0, 8.0, 16.0, 24.0, 32.0]);
    assert_eq!(attributes.character_widths, [8.0; 5]);

    let mut node = Node::new(Role::TextRun);
    attributes.apply(&mut node);
    assert_eq!(node.character_lengths(), [1, 2, 1, 1, 1]);
    assert_eq!(node.text_direction(), Some(TextDirection::LeftToRight));
}

/// The consistency check is not vacuous: a run whose lengths do not sum to its text fails it.
#[test]
fn a_text_run_whose_lengths_do_not_sum_is_rejected() {
    let clusters = [ClusterGeometry {
        text: 0..1,
        offset: CssPx::ZERO,
        advance: CssPx(8.0),
    }];
    let attributes =
        TextRunAttributes::from_clusters(&clusters, Vec::new(), TextDirection::LeftToRight);

    assert!(attributes.is_consistent("a"));
    assert!(
        !attributes.is_consistent("ab"),
        "one cluster is not two bytes"
    );
}
