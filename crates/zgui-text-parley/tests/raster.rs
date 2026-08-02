//! Glyphs into pixels: the three formats, the atlas conversions, and the two paths.

mod support;

use std::sync::Arc;

use zgui_atlas::TextureKind;
use zgui_geom::{DevicePx, Point, Size};
use zgui_interned::Ident;
use zgui_text::kurbo::Shape;
use zgui_text::{
    AtlasGlyph, FaceId, FaceQuery, FontSource, GlyphFormat, GlyphImage, GlyphKey, GlyphRaster,
    OutlineKey, ParagraphShaper, RasterStyle, SubpixelOffset,
};
use zgui_text_parley::{Controls, FontSystem, FontSystemOptions, Rasteriser};
use zgui_text_style::{Direction, FamilyName, FontFamilyList, TextStyle};

/// The face and glyph index of the first glyph a string shapes to.
///
/// Taken from a shaped paragraph rather than looked up directly, because that is the route paint
/// takes: a run names the file its face lives in, and the handle a glyph is cached under comes
/// back through [`FontSystem::face_for`].
fn first_glyph(text: &str) -> (Arc<FontSystem>, FaceId, u16) {
    let (fonts, mut shaper) = support::shaper(Controls::Verbatim);
    let fixture = support::Fixture::new(text, Direction::LeftToRight);
    let shaped = shaper.shape(&fixture.content());
    let line = shaped.engine.layout.get(0).expect("one line");
    let run = line.runs().next().expect("one run");
    let face = fonts.face_for(run.font());
    let glyph = run
        .clusters()
        .flat_map(|cluster| cluster.glyphs().collect::<Vec<_>>())
        .next()
        .expect("one glyph");
    (fonts, face, glyph.id as u16)
}

/// A grayscale glyph rasterises to one coverage byte per pixel, and the same key twice gives the
/// same bytes.
#[test]
fn a_grayscale_glyph_is_one_byte_per_pixel() {
    let (fonts, face, glyph) = first_glyph("g");
    let raster = Rasteriser::new(fonts);

    let key = GlyphKey::new(face, glyph, 32.0, SubpixelOffset(0), RasterStyle::Grayscale);
    let image = raster.raster(&key).expect("a glyph with an outline");
    assert_eq!(image.format, GlyphFormat::Mono);
    assert!(image.is_well_formed());
    assert!(!image.is_empty());
    assert_eq!(
        image.bytes.len(),
        (image.size.width * image.size.height) as usize
    );
    assert_eq!(raster.raster(&key).as_ref(), Some(&image));
}

/// A subpixel glyph is three coverage values per pixel, not the four the rasteriser reports.
#[test]
fn a_subpixel_glyph_is_three_bytes_per_pixel() {
    let (fonts, face, glyph) = first_glyph("g");
    let raster = Rasteriser::new(fonts);
    let image = raster
        .raster(&GlyphKey::new(
            face,
            glyph,
            32.0,
            SubpixelOffset(0),
            RasterStyle::Subpixel,
        ))
        .expect("rasterises");

    assert_eq!(image.format, GlyphFormat::Subpixel);
    assert!(image.is_well_formed());
    assert!(!image.is_empty());
    assert_eq!(
        image.bytes.len(),
        (image.size.width * image.size.height) as usize * 3,
        "the padding byte the rasteriser reports is dropped here rather than uploaded"
    );
}

/// The four subpixel phases are four different rasterisations, which is what makes the phase in
/// the key load-bearing rather than decorative.
#[test]
fn the_subpixel_phase_changes_the_pixels() {
    let (fonts, face, glyph) = first_glyph("n");
    let raster = Rasteriser::new(fonts);
    let at = |phase: u8| {
        raster
            .raster(&GlyphKey::new(
                face,
                glyph,
                24.0,
                SubpixelOffset(phase),
                RasterStyle::Grayscale,
            ))
            .expect("rasterises")
    };
    let (upright, shifted) = (at(0), at(2));
    assert_ne!(
        (upright.bytes, upright.size),
        (shifted.bytes, shifted.size),
        "half a pixel of horizontal shift must change the coverage"
    );
}

/// A colour face rasterises to straight-alpha colour, and premultiplies on the way into a tile.
#[test]
fn a_colour_glyph_takes_the_colour_pool() {
    let fonts = Arc::new(FontSystem::new(FontSystemOptions::registered_only()));
    fonts
        .register(
            support::face("NotoZnamennyMusicalNotation-Regular.ttf"),
            None,
        )
        .expect("registers");
    let style = TextStyle {
        family: FontFamilyList::from_iter([FamilyName::Named(Ident::new(support::COLOR))]),
        ..TextStyle::initial()
    };
    let face = fonts
        .resolve(&FaceQuery::of(&style))
        .expect("the colour face resolves");
    assert!(fonts.face(face).expect("described").has_color);

    let raster = Rasteriser::new(fonts);
    let (key, image) = (1..64u16)
        .find_map(|glyph| {
            let key = GlyphKey::new(face, glyph, 48.0, SubpixelOffset(0), RasterStyle::Color);
            let image = raster.raster(&key)?;
            (image.format == GlyphFormat::Color && !image.is_empty()).then_some((key, image))
        })
        .expect("the face draws at least one glyph in colour");

    assert!(image.is_well_formed());
    let tile = AtlasGlyph::of(&key, &image);
    assert_eq!(tile.key.kind(), TextureKind::Color);
    assert_eq!(tile.texels.len(), image.bytes.len());
    assert!(
        image
            .bytes
            .chunks_exact(4)
            .zip(tile.texels.chunks_exact(4))
            .any(|(straight, premultiplied)| straight[..3] != premultiplied[..3]),
        "at least one texel must actually have been premultiplied"
    );
    for pixel in tile.texels.chunks_exact(4) {
        assert!(
            pixel[0] <= pixel[3] && pixel[1] <= pixel[3] && pixel[2] <= pixel[3],
            "a premultiplied texel's colour cannot exceed its alpha"
        );
    }
}

/// The three glyph formats land in the three atlas pools, and the subpixel one is widened.
#[test]
fn every_format_has_a_pool() {
    let key = GlyphKey::new(FaceId(0), 1, 16.0, SubpixelOffset(0), RasterStyle::Subpixel);
    let image = GlyphImage {
        size: Size::new(2, 1),
        placement: Point::new(DevicePx(0.0), DevicePx(0.0)),
        format: GlyphFormat::Subpixel,
        bytes: vec![10, 20, 30, 40, 50, 60],
    };
    let tile = AtlasGlyph::of(&key, &image);
    assert_eq!(tile.key.kind(), TextureKind::Subpixel);
    assert_eq!(tile.texels, vec![10, 20, 30, 255, 40, 50, 60, 255]);
    assert_eq!(
        tile.texels.len() as u32,
        tile.size.width * tile.size.height * TextureKind::Subpixel.format().bytes_per_texel()
    );

    let mono = GlyphImage {
        format: GlyphFormat::Mono,
        bytes: vec![7, 8],
        ..image
    };
    let tile = AtlasGlyph::of(&key, &mono);
    assert_eq!(
        tile.key.kind(),
        TextureKind::Mono,
        "coverage stays one byte per texel, which is what keeps a text frame's uploads small"
    );
    assert_eq!(tile.texels, vec![7, 8]);
}

/// The curves of a glyph are the same shape the tile is, in the space outlines are defined in.
///
/// This is what says the outline path draws the *glyph* rather than a plausible blob: the bitmap
/// rasteriser reports where the ink sits relative to the origin, and the curves have to agree to
/// within the pixel the two round differently by.
#[test]
fn a_glyphs_curves_cover_the_same_ink_its_tile_does() {
    let (fonts, face, glyph) = first_glyph("H");
    let raster = Rasteriser::new(fonts);
    let size = 64.0;

    let image = raster
        .raster(&GlyphKey::new(
            face,
            glyph,
            size,
            SubpixelOffset(0),
            RasterStyle::Grayscale,
        ))
        .expect("a glyph with an outline");
    let curves = raster
        .outline(&OutlineKey::new(face, glyph, size))
        .expect("the same glyph has curves");
    let bounds = curves.bounding_box();

    // The image's placement is measured rightwards and upwards from the origin; the curves are in
    // the surface's space, where up is negative.
    let expected_top = f64::from(-image.placement.y.0);
    let expected_left = f64::from(image.placement.x.0);
    assert!(
        (bounds.y0 - expected_top).abs() <= 1.5,
        "the curves start where the pixels do: {bounds:?} against a placement of {:?}",
        image.placement
    );
    assert!(
        (bounds.x0 - expected_left).abs() <= 1.5,
        "the curves start where the pixels do: {bounds:?} against a placement of {:?}",
        image.placement
    );
    assert!(
        (bounds.width() - f64::from(image.size.width)).abs() <= 2.0
            && (bounds.height() - f64::from(image.size.height)).abs() <= 2.0,
        "the curves are the size the pixels are: {bounds:?} against {:?}",
        image.size
    );
}

/// Curves at twice the size are twice as big — the size is in the key and reaches the face.
#[test]
fn curves_are_extracted_at_the_size_they_are_asked_for() {
    let (fonts, face, glyph) = first_glyph("H");
    let raster = Rasteriser::new(fonts);
    let small = raster
        .outline(&OutlineKey::new(face, glyph, 32.0))
        .expect("curves");
    let large = raster
        .outline(&OutlineKey::new(face, glyph, 128.0))
        .expect("curves");
    let ratio = large.bounding_box().height() / small.bounding_box().height();
    assert!(
        (ratio - 4.0).abs() < 0.05,
        "four times the size is four times the height, and this was {ratio}"
    );
}

/// The same key twice is the same allocation, which is what a rasteriser's encoding cache is keyed
/// on: a fresh copy per frame would re-encode every glyph of every turned heading.
#[test]
fn the_same_curves_are_handed_back_rather_than_rebuilt() {
    let (fonts, face, glyph) = first_glyph("g");
    let raster = Rasteriser::new(fonts);
    let key = OutlineKey::new(face, glyph, 96.0);
    let first = raster.outline(&key).expect("curves");
    let second = raster.outline(&key).expect("curves");
    assert!(
        std::sync::Arc::ptr_eq(&first, &second),
        "two calls handed out two allocations of the same shape"
    );
}

/// A synthesised italic leans the letter and not the line, so it is part of the curve.
#[test]
fn a_synthetic_slant_shears_the_curves_about_the_baseline() {
    let (fonts, face, glyph) = first_glyph("l");
    let raster = Rasteriser::new(fonts);
    let upright = raster
        .outline(&OutlineKey::new(face, glyph, 96.0))
        .expect("curves");
    let leaning = raster
        .outline(&OutlineKey {
            synthetic_slant_bits: 14.0f32.to_bits(),
            ..OutlineKey::new(face, glyph, 96.0)
        })
        .expect("curves");
    assert!(
        leaning.bounding_box().x1 > upright.bounding_box().x1 + 5.0,
        "a leaning letter reaches further right at the top: {:?} against {:?}",
        leaning.bounding_box(),
        upright.bounding_box()
    );
    assert!(
        (leaning.bounding_box().y1 - upright.bounding_box().y1).abs() < 0.001,
        "the shear is about the baseline, so nothing on the baseline moves"
    );
}

/// A face nobody registered has no curves, which is not the same answer as a blank glyph.
#[test]
fn an_unknown_face_has_no_curves() {
    let fonts = support::fonts();
    let raster = Rasteriser::new(fonts);
    assert!(
        raster
            .outline(&OutlineKey::new(FaceId(9_999), 1, 32.0))
            .is_none()
    );
}

/// A handle no system issued has no glyph, which is not the same answer as a blank one.
#[test]
fn an_unknown_face_has_no_glyphs() {
    let fonts = support::fonts();
    let raster = Rasteriser::new(fonts);
    let key = GlyphKey::new(
        FaceId(9_999),
        1,
        16.0,
        SubpixelOffset(0),
        RasterStyle::Grayscale,
    );
    assert!(raster.raster(&key).is_none());
}

/// The same face reached twice is the same handle, so its glyphs share cache entries.
#[test]
fn one_face_has_one_handle() {
    let (fonts, face, _) = first_glyph("g");
    let style = TextStyle {
        family: FontFamilyList::from_iter([FamilyName::Named(Ident::new(support::LATIN))]),
        ..TextStyle::initial()
    };
    assert_eq!(fonts.resolve(&FaceQuery::of(&style)), Some(face));
}

/// A handle keeps naming the same bytes after its family is unregistered, which is the invariant
/// the rasteriser's face cache rests on: without it, holding a lookup across an unregistration
/// would draw one face's glyphs for another's handle.
#[test]
fn a_face_handle_still_rasterises_after_its_family_is_unregistered() {
    let (fonts, face, glyph) = first_glyph("g");
    let raster = Rasteriser::new(fonts.clone());
    let key = GlyphKey::new(face, glyph, 32.0, SubpixelOffset(0), RasterStyle::Grayscale);

    let before = raster.raster(&key).expect("the face is registered");

    fonts.unregister(Ident::new(support::LATIN));

    // The control first: the family really is gone, so this is not a case where nothing happened.
    let mut style = TextStyle::initial();
    style.family = FontFamilyList::from_iter([FamilyName::Named(Ident::new(support::LATIN))]);
    assert_eq!(
        fonts.resolve(&FaceQuery::of(&style)),
        None,
        "the unregistered family must no longer be reachable by name"
    );

    assert_eq!(
        raster.raster(&key).as_ref(),
        Some(&before),
        "a paragraph shaped before the unregistration is still on screen and still has to draw"
    );
}
