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
    let line = shaped
        .engine
        .layout()
        .expect("a live form")
        .get(0)
        .expect("one line");
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

/// Where the ink of the rows from `rows` sits, as the mean column weighted by coverage.
fn ink_column(image: &zgui_text::GlyphImage, rows: std::ops::Range<u32>) -> f32 {
    let width = image.size.width;
    let (mut sum, mut weight) = (0.0f32, 0.0f32);
    for row in rows {
        for column in 0..width {
            let coverage = f32::from(image.bytes[(row * width + column) as usize]);
            sum += coverage * column as f32;
            weight += coverage;
        }
    }
    sum / weight.max(1.0)
}

/// A synthesised italic leans the tile forward: the top of a stem sits right of its foot.
#[test]
fn a_synthetic_slant_leans_the_tile_forward() {
    let (fonts, face, glyph) = first_glyph("l");
    let raster = Rasteriser::new(fonts);
    let key = GlyphKey {
        synthetic_slant_bits: 14.0f32.to_bits(),
        ..GlyphKey::new(face, glyph, 24.0, SubpixelOffset(0), RasterStyle::Grayscale)
    };
    let image = raster.raster(&key).expect("a tile");
    let height = image.size.height;
    let top = ink_column(&image, 0..height / 4);
    let foot = ink_column(&image, height - height / 4..height);
    assert!(
        top > foot + 2.0,
        "the top of the stem leans right of its foot: {top} against {foot}"
    );
}

/// The first run of `text` shaped in the shipped variable face at `weight`, with the system it
/// was shaped through.
fn variable_run(fonts: &Arc<FontSystem>, weight: f32) -> zgui_text::ShapedRunOwned {
    use zgui_text::{BreakRequest, ShapedRunOwned};

    let mut shaper = zgui_text_parley::Shaper::with_controls(fonts.clone(), Controls::Verbatim);
    let mut fixture = support::Fixture::new("HOW", Direction::LeftToRight);
    fixture.runs[0].style = Arc::new(TextStyle {
        family: FontFamilyList::from_iter([FamilyName::Named(Ident::new("Rubik"))]),
        size: zgui_geom::CssPx(48.0),
        weight,
        ..TextStyle::initial()
    });
    let content = fixture.content();
    let mut shaped = shaper.shape(&content);
    shaper.break_lines(&mut shaped, &BreakRequest::new(&content, None));
    let mut runs = Vec::new();
    shaper.visit_line(&shaped, 0, &mut |run| runs.push(ShapedRunOwned::from(run)));
    runs.into_iter().next().expect("one run")
}

/// The pixels a glyph covers, summed.
fn ink(image: &GlyphImage) -> u64 {
    image.bytes.iter().map(|byte| u64::from(*byte)).sum()
}

/// A variable face draws each weight at its own point on the `wght` axis: the runs name two
/// instances, the glyphs are two cache entries, and the heavier one carries more ink.
#[test]
fn a_variable_face_draws_each_weight_at_its_own_instance() {
    let fonts = Arc::new(FontSystem::new(FontSystemOptions::registered_only()));
    fonts
        .register(support::face("RubikSubset-Variable.ttf"), None)
        .expect("the variable face registers");
    let regular = variable_run(&fonts, 400.0);
    let bold = variable_run(&fonts, 700.0);
    assert_eq!(regular.face, bold.face, "one file, one face");
    assert_ne!(
        regular.instance, bold.instance,
        "two weights of one variable face were shaped at one instance"
    );
    assert!(!bold.instance.is_default());
    assert_eq!(bold.synthetic_bold, 0.0, "the weight comes from the axis");

    let raster = Rasteriser::new(fonts);
    let glyph = regular.glyphs[0];
    let style = RasterStyle::Grayscale;
    let light_key = regular
        .as_run(zgui_scene::PaintSlot(0))
        .key_for(glyph, 0.0, style);
    let heavy_key = bold
        .as_run(zgui_scene::PaintSlot(0))
        .key_for(glyph, 0.0, style);
    assert_ne!(light_key, heavy_key);
    let light = raster.raster(&light_key).expect("a glyph");
    let heavy = raster.raster(&heavy_key).expect("a glyph");
    assert!(
        ink(&heavy) > ink(&light) * 11 / 10,
        "700 drew no heavier than 400: {} against {}",
        ink(&heavy),
        ink(&light)
    );

    let light_curves = raster
        .outline(
            &regular
                .as_run(zgui_scene::PaintSlot(0))
                .outline_key(glyph.glyph),
        )
        .expect("curves");
    let heavy_curves = raster
        .outline(
            &bold
                .as_run(zgui_scene::PaintSlot(0))
                .outline_key(glyph.glyph),
        )
        .expect("curves");
    assert!(
        heavy_curves.bounding_box().width() > light_curves.bounding_box().width(),
        "the heavier curves are no wider: {:?} against {:?}",
        heavy_curves.bounding_box(),
        light_curves.bounding_box()
    );
}

/// A static face is drawn at the default instance, so its keys are the ones it always had.
#[test]
fn a_static_face_is_drawn_at_the_default_instance() {
    let (fonts, mut shaper) = support::shaper(Controls::Verbatim);
    let fixture = support::Fixture::new("H", Direction::LeftToRight);
    let content = fixture.content();
    let mut shaped = shaper.shape(&content);
    shaper.break_lines(&mut shaped, &zgui_text::BreakRequest::new(&content, None));
    let mut instances = Vec::new();
    shaper.visit_line(&shaped, 0, &mut |run| instances.push(run.instance));
    let _ = fonts;
    assert_eq!(instances, vec![zgui_text::VariationInstance::DEFAULT]);
}
