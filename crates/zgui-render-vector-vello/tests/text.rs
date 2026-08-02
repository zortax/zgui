//! What text that leaves the atlas actually draws, on a real device.
//!
//! Every case here runs the whole chain: a real face, shaped by the real engine, promoted by the
//! real paint stage because of what the run and the surface are, extracted as curves by the real
//! font engine, rasterised by this crate, and read back as pixels. Nothing here asserts that a
//! primitive was emitted — a display list full of vector items composites a scratch nobody wrote
//! just as happily as one that draws, and that is precisely the failure this file exists to catch.

mod support;

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_css::StyleDraft;
use zgui_geom::{Affine2, CssPx, DevicePx, Point, Rect, Scale, Size};
use zgui_layout::Paragraphs;
use zgui_layout::fragment::ParagraphId;
use zgui_layout::measure::MeasureContent;
use zgui_layout::tree::store::LayoutStore;
use zgui_paint::emit::text::{TextPlacement, emit};
use zgui_paint::{ContentCache, GlyphSource};
use zgui_render::{RenderTarget, Renderer};
use zgui_render_wgpu::{Builder, Pixels, WgpuRenderer, wgpu};
use zgui_scene::{ClipId, Scene};
use zgui_text::{FontSource, ParagraphContent, StyledRun, TextMap};
use zgui_text_parley::{FontSystem, FontSystemOptions, Rasteriser, Shaper};
use zgui_text_style::{
    Direction, FamilyName, FontFamilyList, ParagraphStyle, TextAlign, TextStyle,
};

use support::{Harness, SIDE, Which, difference, harness_at, present, twins, whole_pixels};

/// The extent every case here draws into.
const EXTENT: i32 = 256;

/// The face these cases are drawn in, which ships with the text engine's own tests.
const FACE: &str = "Noto Sans";

/// The colour face shipped with the text engine's tests, which carries layered colour outlines.
const COLOR_FACE: &str = "Noto Znamenny Musical Notation";

/// Reads one of the faces shipped with the text engine's own tests.
fn face(file: &str) -> Vec<u8> {
    let path = format!(
        "{}/../zgui-text-parley/tests/fonts/{file}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|error| panic!("reading {path}: {error}"))
}

/// The font system holding the shipped faces and nothing the machine happens to have.
fn fonts() -> Arc<FontSystem> {
    let system = Arc::new(FontSystem::new(FontSystemOptions::registered_only()));
    system
        .register(Arc::new(face("NotoSans-Regular.ttf")), None)
        .expect("the Latin face registers");
    system
        .register(
            Arc::new(face("NotoZnamennyMusicalNotation-Regular.ttf")),
            None,
        )
        .expect("the colour face registers");
    system
}

/// How one case's text is drawn.
#[derive(Clone, Copy)]
struct Drawn {
    /// The size in CSS pixels.
    size: f32,
    /// Where the line box's top-left corner goes.
    origin: (f32, f32),
    /// The transform in force, about the surface's origin.
    transform: Affine2,
    /// Whether the run is painted with a ramp across the line rather than one colour.
    gradient: bool,
    /// The family the run is shaped in.
    family: &'static str,
}

impl Drawn {
    /// Upright black text of the given size at the given corner.
    fn plain(size: f32, origin: (f32, f32)) -> Self {
        Self {
            size,
            origin,
            transform: Affine2::IDENTITY,
            gradient: false,
            family: FACE,
        }
    }

    /// The same, in the face whose glyphs are pictures rather than outlines.
    fn in_colour(self) -> Self {
        Self {
            family: COLOR_FACE,
            ..self
        }
    }

    /// The same, turned by `degrees` about the line box's own origin.
    fn turned(self, degrees: f32) -> Self {
        let (x, y) = self.origin;
        let about = Affine2::translation(-x, -y)
            .then(Affine2::rotation(degrees.to_radians()))
            .then(Affine2::translation(x, y));
        Self {
            transform: about,
            ..self
        }
    }

    /// The same, under an arbitrary linear transform about the line box's own origin.
    fn under(self, linear: Affine2) -> Self {
        let (x, y) = self.origin;
        Self {
            transform: Affine2::translation(-x, -y)
                .then(linear)
                .then(Affine2::translation(x, y)),
            ..self
        }
    }

    /// The same, painted with a ramp across the line box.
    fn with_gradient(self) -> Self {
        Self {
            gradient: true,
            ..self
        }
    }
}

/// Builds the display list one string is drawn through, and says how it was drawn.
///
/// The stages are the real ones and are wired in the real order: shaping, then the paint stage's
/// own glyph source — which is what decides between tiles and curves — then the emitter.
fn scene_of(text: &str, drawn: Drawn) -> Scene {
    let fonts = fonts();
    let mut paragraphs = Paragraphs::new(Shaper::new(Arc::clone(&fonts)));

    let mut map = TextMap::new();
    map.push(0..text.len(), 0, 0);
    let style = Arc::new(TextStyle {
        family: FontFamilyList::from_iter([FamilyName::Named(zgui_interned::Ident::new(
            drawn.family,
        ))]),
        size: CssPx(drawn.size),
        ..TextStyle::initial()
    });
    let runs = vec![StyledRun {
        text: 0..text.len(),
        style,
        brush: zgui_scene::PaintSlot(0),
    }];
    let paragraph_style = ParagraphStyle {
        direction: Direction::LeftToRight,
        align: TextAlign::Start,
        ..ParagraphStyle::initial()
    };
    let summary = paragraphs.shape(&ParagraphContent {
        text,
        map: &map,
        runs: &runs,
        boxes: &[],
        paragraph: &paragraph_style,
        scale: 1.0,
    });

    let mut store = LayoutStore::new(zgui_arena::DocumentId::FIRST);
    let paragraph = store.intern_paragraph(summary.key);

    let mut scene = Scene::new();
    scene.begin_frame(Size::new(EXTENT, EXTENT));
    let transform = if drawn.transform == Affine2::IDENTITY {
        scene.spatial.viewport()
    } else {
        space(&mut scene, 2, drawn.transform.to_matrix4())
    };

    let mut lowered = zgui_paint::lower::lower(&StyleDraft::initial().build(), 1.0);
    lowered.color = Color::srgb(0.0, 0.0, 0.0, 1.0);
    if drawn.gradient {
        lowered.text_fill = Some(ramp());
    }

    let raster = Rasteriser::new(fonts);
    let mut cache = ContentCache::new(zgui_atlas::AtlasLimits::default());
    {
        let content = cache.frame(&store, &paragraphs, &raster);
        emit(
            &mut scene,
            &content as &dyn GlyphSource,
            ParagraphId(paragraph.0),
            0,
            &lowered,
            // The ramp is propagated down the emit walk rather than read off the line's own style,
            // because a line box belongs to an anonymous box below whatever declared it. This is
            // that walk's answer, handed in directly.
            zgui_paint::emit::text::Inherited {
                text_fill: lowered.text_fill.as_ref(),
                decorations: &[],
            },
            TextPlacement {
                line: Rect::new(
                    Point::new(DevicePx(drawn.origin.0), DevicePx(drawn.origin.1)),
                    Size::new(DevicePx(200.0), DevicePx(drawn.size * 1.4)),
                ),
                clip: ClipId::ROOT,
                transform,
                opaque_target: true,
                subpixel_capable: false,
                upright: true,
                scale: 1.0,
            },
        );
    }
    scene.finish(&DamageSet::full());
    scene
}

/// A red-to-blue ramp across the line box.
fn ramp() -> zgui_paint::lower::background::GradientSpec {
    use zgui_paint::lower::background::{GradientShape, GradientSpec, SpecStop};
    GradientSpec {
        // Ninety degrees clockwise from twelve o'clock: left to right.
        shape: GradientShape::Linear {
            angle: std::f32::consts::FRAC_PI_2,
        },
        stops: [
            SpecStop {
                color: Color::srgb(1.0, 0.0, 0.0, 1.0),
                position: None,
            },
            SpecStop {
                color: Color::srgb(0.0, 0.0, 1.0, 1.0),
                position: None,
            },
        ]
        .into_iter()
        .collect(),
        interpolation: zgui_color::Interpolation::new(zgui_color::ColorSpace::Srgb),
        repeating: false,
    }
}

/// A renderer at this file's extent, with the compute-shader rasteriser attached.
fn renderer() -> Option<Harness> {
    let _ = SIDE;
    harness_at(EXTENT, Which::Vello)
}

/// Where the ink is: every pixel that is not the background.
fn inked(pixels: &Pixels) -> Vec<(i32, i32)> {
    let mut found = Vec::new();
    for y in 0..EXTENT {
        for x in 0..EXTENT {
            if pixels.rgba(x, y)[3] > 32 {
                found.push((x, y));
            }
        }
    }
    found
}

/// The smallest rectangle containing every inked pixel, as left, top, right, bottom.
fn ink_extent(pixels: &Pixels) -> Option<(i32, i32, i32, i32)> {
    let ink = inked(pixels);
    let left = ink.iter().map(|(x, _)| *x).min()?;
    let right = ink.iter().map(|(x, _)| *x).max()?;
    let top = ink.iter().map(|(_, y)| *y).min()?;
    let bottom = ink.iter().map(|(_, y)| *y).max()?;
    Some((left, top, right, bottom))
}

/// How many primitives of each kind a display list carries.
fn counts(scene: &Scene) -> (usize, usize) {
    let sprites = scene.primitives.mono_sprites.len()
        + scene.primitives.subpixel_sprites.len()
        + scene.primitives.color_sprites.len();
    (sprites, scene.primitives.vectors.len())
}

/// A display-sized run leaves the atlas, and the curves it leaves as reach the screen.
///
/// The counterfactual is the whole test: a run promoted to curves that nothing rasterised
/// composites a scratch nobody wrote, which is a display list that looks perfect and a surface with
/// nothing on it.
#[test]
fn a_run_too_large_for_the_atlas_is_drawn_as_curves() {
    let Some(mut harness) = renderer() else {
        return;
    };
    let scene = scene_of("Hlm", Drawn::plain(120.0, (20.0, 30.0)));
    let (sprites, vectors) = counts(&scene);
    assert_eq!(sprites, 0, "a run above the atlas limit allocates no tiles");
    assert!(
        vectors >= 3,
        "one filled curve per glyph, and there are three"
    );

    let pixels = present(&mut harness, &scene);
    let extent = ink_extent(&pixels).expect("a display-sized word draws pixels");
    let (left, top, right, bottom) = extent;
    assert!(
        right - left > 100 && bottom - top > 50,
        "three letters at 120 pixels cover a large rectangle, and this was {extent:?}"
    );
    assert!(
        top >= 30 && bottom <= 30 + (120.0 * 1.4) as i32,
        "the ink sits inside the line box it was placed in: {extent:?}"
    );
}

/// The same string at zero and ninety degrees has transposed extents.
///
/// Not "some ink somewhere": the width of one is the height of the other, to within a pixel of
/// antialiasing, which is only true if the curves themselves turned.
#[test]
fn the_same_string_at_zero_and_ninety_degrees_has_transposed_ink() {
    let Some(mut harness) = renderer() else {
        return;
    };
    // Turned a quarter turn about its own origin, the run sweeps up and to the left, so the
    // origin sits far enough right that both renderings are wholly on the surface — an extent
    // clipped by the edge would be an extent this case could not compare.
    let upright = Drawn::plain(110.0, (140.0, 30.0));
    let flat = present(&mut harness, &scene_of("Hl", upright));
    let turned = present(&mut harness, &scene_of("Hl", upright.turned(90.0)));

    let (fl, ft, fr, fb) = ink_extent(&flat).expect("the upright string draws pixels");
    let (tl, tt, tr, tb) = ink_extent(&turned).expect("the turned string draws pixels");
    let (flat_width, flat_height) = (fr - fl, fb - ft);
    let (turned_width, turned_height) = (tr - tl, tb - tt);

    assert!(
        flat_width > flat_height,
        "the upright string is wider than it is tall: {flat_width} by {flat_height}"
    );
    assert!(
        (turned_width - flat_height).abs() <= 2 && (turned_height - flat_width).abs() <= 2,
        "a quarter turn transposes the extents: {flat_width} by {flat_height} became \
         {turned_width} by {turned_height}"
    );
}

/// Rotated text has its ink along the rotated baseline and nothing along the upright one.
///
/// The second half is what fails for a run that never left the atlas in the first place, and for
/// one whose curves were placed before the transform rather than under it.
#[test]
fn a_turned_run_puts_its_ink_along_the_turned_baseline() {
    let Some(mut harness) = renderer() else {
        return;
    };
    let degrees = 30.0_f32;
    let origin = (30.0, 40.0);
    let pixels = present(
        &mut harness,
        &scene_of("HHHHH", Drawn::plain(40.0, origin).turned(degrees)),
    );
    let ink = inked(&pixels);
    assert!(!ink.is_empty(), "a turned word draws pixels");

    // The baseline of an upright run sits one ascent below the line box's top; the run's ink hugs
    // it. Under the rotation both the baseline and the ink turn about the line box's origin, so
    // every inked pixel has to be within a letter's height of the turned line.
    let (sin, cos) = degrees.to_radians().sin_cos();
    let along = |x: f32, y: f32| {
        let (dx, dy) = (x - origin.0, y - origin.1);
        // Distance from the turned x axis through the line box's origin.
        (-dx * sin + dy * cos).abs()
    };
    let furthest = ink
        .iter()
        .map(|(x, y)| along(*x as f32, *y as f32))
        .fold(0.0_f32, f32::max);
    assert!(
        furthest < 60.0,
        "every inked pixel is near the turned baseline, and one was {furthest} away"
    );

    // And the ink is genuinely off the upright baseline: the further along the run, the further
    // down the surface. A row of upright sprites in a turned box would keep its ink flat.
    let leftmost = ink.iter().map(|(x, _)| *x).min().expect("ink");
    let rightmost = ink.iter().map(|(x, _)| *x).max().expect("ink");
    let mean_y = |column: i32| {
        let rows: Vec<i32> = ink
            .iter()
            .filter(|(x, _)| (*x - column).abs() <= 2)
            .map(|(_, y)| *y)
            .collect();
        rows.iter().sum::<i32>() as f32 / rows.len().max(1) as f32
    };
    let rise = mean_y(rightmost - 4) - mean_y(leftmost + 4);
    let run = (rightmost - leftmost - 8) as f32;
    let slope = rise / run;
    assert!(
        (slope - degrees.to_radians().tan()).abs() < 0.15,
        "the ink descends at the angle it was turned by: slope {slope} against {}",
        degrees.to_radians().tan()
    );
}

/// A skew and a non-uniform scale reach the pixels too, and each in its own way.
#[test]
fn a_skew_leans_the_ink_and_a_non_uniform_scale_stretches_it() {
    let Some(mut harness) = renderer() else {
        return;
    };
    let base = Drawn::plain(100.0, (40.0, 40.0));
    let upright = ink_extent(&present(&mut harness, &scene_of("H", base))).expect("ink");
    let sheared = ink_extent(&present(
        &mut harness,
        &scene_of("H", base.under(Affine2::skew(-0.4, 0.0))),
    ))
    .expect("ink");
    let stretched = ink_extent(&present(
        &mut harness,
        &scene_of("H", base.under(Affine2::scale(1.0, 2.0))),
    ))
    .expect("ink");

    let width = |extent: (i32, i32, i32, i32)| extent.2 - extent.0;
    let height = |extent: (i32, i32, i32, i32)| extent.3 - extent.1;
    assert!(
        width(sheared) > width(upright) + 20,
        "a shear widens a letter without making it taller: {sheared:?} against {upright:?}"
    );
    assert!(
        (height(sheared) - height(upright)).abs() <= 2,
        "a horizontal shear leaves the height alone: {sheared:?} against {upright:?}"
    );
    assert!(
        (height(stretched) as f32 / height(upright) as f32 - 2.0).abs() < 0.1,
        "doubling y doubles the ink's height: {stretched:?} against {upright:?}"
    );
    assert!(
        (width(stretched) - width(upright)).abs() <= 2,
        "and leaves its width alone: {stretched:?} against {upright:?}"
    );
}

/// A ramp across a run is a ramp across the letters, not a colour per letter.
#[test]
fn a_gradient_brush_runs_across_the_glyphs() {
    let Some(mut harness) = renderer() else {
        return;
    };
    // Sixteen pixels: small enough that the size alone would keep it on the atlas, so the brush is
    // the only thing that can have promoted it.
    let drawn = Drawn::plain(16.0, (20.0, 20.0)).with_gradient();
    let scene = scene_of("HHHHHHHHHHHH", drawn);
    let (sprites, vectors) = counts(&scene);
    assert_eq!(sprites, 0, "a run painted with a ramp leaves the atlas");
    assert!(vectors > 0, "and is drawn as curves");

    let pixels = present(&mut harness, &scene);
    let ink = inked(&pixels);
    assert!(!ink.is_empty(), "the run draws pixels");
    let leftmost = ink.iter().map(|(x, _)| *x).min().expect("ink");
    let rightmost = ink.iter().map(|(x, _)| *x).max().expect("ink");
    assert!(
        rightmost - leftmost > 40,
        "a dozen letters cover a good stretch of the line"
    );

    let redness = |column: i32| {
        let sampled: Vec<[u8; 4]> = ink
            .iter()
            .filter(|(x, _)| (*x - column).abs() <= 1)
            .map(|(x, y)| pixels.rgba(*x, *y))
            .filter(|rgba| rgba[3] > 200)
            .collect();
        let count = sampled.len().max(1) as f32;
        let red: f32 = sampled.iter().map(|rgba| f32::from(rgba[0])).sum();
        let blue: f32 = sampled.iter().map(|rgba| f32::from(rgba[2])).sum();
        (red / count, blue / count)
    };
    let (left_red, left_blue) = redness(leftmost + 2);
    let (right_red, right_blue) = redness(rightmost - 2);
    assert!(
        left_red > right_red + 60.0 && right_blue > left_blue + 60.0,
        "the ramp runs from red to blue across the line: left ({left_red}, {left_blue}), right \
         ({right_red}, {right_blue})"
    );
}

/// Ordinary text stays on the atlas, at its own size, upright, in one colour.
///
/// The promotion has to be a rule and not a preference: a component library is almost entirely
/// this case, and drawing it as curves would cost every label its hinting and its cached tile.
#[test]
fn ordinary_text_is_not_promoted() {
    let scene = scene_of("Ordinary body text", Drawn::plain(16.0, (10.0, 10.0)));
    let (sprites, vectors) = counts(&scene);
    assert!(sprites > 0, "sixteen-pixel upright text is drawn as tiles");
    assert_eq!(vectors, 0, "and no curve is filled for it");
}

/// The same display list through a renderer with no rasteriser attached draws nothing.
///
/// This is the shape of the defect this whole file is written against: the display list is
/// identical, every count is identical, and the surface is empty.
#[test]
fn without_a_rasteriser_the_same_display_list_draws_nothing() {
    let Some(mut attached) = renderer() else {
        return;
    };
    let scene = scene_of("Hlm", Drawn::plain(120.0, (20.0, 30.0)));
    let drawn = present(&mut attached, &scene);
    assert!(!inked(&drawn).is_empty(), "with a rasteriser, there is ink");

    let target = RenderTarget::new(Size::new(EXTENT, EXTENT), Scale::new(1.0));
    let mut bare = match Builder::new().offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false) {
        Ok(renderer) => renderer,
        Err(_) => return,
    };
    assert!(!bare.has_vector_raster());
    let blank = present_unattached(&mut bare, &scene);
    assert!(
        inked(&blank).is_empty(),
        "with none, the very same display list composites an empty scratch"
    );
}

/// A colour face is never promoted, however large and however turned.
///
/// Its glyphs are pictures — layered outlines, or a bitmap strike — and there is no single outline
/// to fill: what the vector path would draw is the first layer's silhouette in the text colour.
#[test]
fn a_colour_run_stays_on_the_atlas() {
    let drawn = Drawn::plain(200.0, (20.0, 20.0)).in_colour().turned(25.0);
    let scene = scene_of("\u{1f480}\u{e000}A", drawn);
    let (sprites, vectors) = counts(&scene);
    assert!(
        sprites > 0,
        "a colour run is drawn as tiles at any size under any transform"
    );
    assert_eq!(vectors, 0, "and never as curves");
}

/// Every pixel a turned run draws lies inside the ink its display list reported.
///
/// This is the damage contract, and it is where an outline run differs from the atlas run it
/// replaced: the rectangle a turned glyph paints is not the rectangle it would have occupied
/// upright, so an ink measured before the transform under-reports exactly the pixels that moved —
/// and the same under-report is what sizes the scratch the curves are rasterised into, so the
/// letters would be cut off as well as left behind.
#[test]
fn the_reported_ink_contains_every_pixel_a_turned_run_draws() {
    let Some(mut harness) = renderer() else {
        return;
    };
    let drawn = Drawn::plain(100.0, (60.0, 80.0)).turned(35.0);
    let scene = scene_of("Hl", drawn);
    let reported = scene
        .primitives
        .vectors
        .iter()
        .map(|item| item.ink)
        .reduce(|union, ink| union.union(ink))
        .expect("a turned run reports ink");

    let pixels = present(&mut harness, &scene);
    let ink = inked(&pixels);
    assert!(!ink.is_empty(), "the turned run draws pixels");
    for (x, y) in &ink {
        let inside = f32::from(*x as u16) >= reported.origin.x.0 - 1.0
            && f32::from(*x as u16) <= reported.origin.x.0 + reported.size.width.0 + 1.0
            && f32::from(*y as u16) >= reported.origin.y.0 - 1.0
            && f32::from(*y as u16) <= reported.origin.y.0 + reported.size.height.0 + 1.0;
        assert!(
            inside,
            "a pixel at ({x}, {y}) was drawn outside the reported ink {reported:?}"
        );
    }

    // And the reported rectangle is the turned one rather than the upright one it would have been:
    // a 35-degree turn makes a wide, short line taller than it was.
    let upright = scene_of("Hl", Drawn::plain(100.0, (60.0, 80.0)));
    let flat = upright
        .primitives
        .vectors
        .iter()
        .map(|item| item.ink)
        .reduce(|union, ink| union.union(ink))
        .expect("the upright run reports ink");
    assert!(
        reported.size.height.0 > flat.size.height.0 + 20.0,
        "the ink turned with the text: {reported:?} against {flat:?}"
    );
}

/// The fallback rasteriser turns the same letters the same way.
///
/// The seam this architecture is arranged around. The paint stage decides that a run is curves and
/// where those curves go; which rasteriser fills them is a property of the device. A promotion that
/// only drew on the compute-shader path would be text that vanishes on a device without one — so
/// the transposition is asserted against the fallback in its own terms, on its own pixels.
#[test]
fn an_outline_run_is_turned_the_same_way_by_the_fallback_rasteriser() {
    let Some(mut harness) = harness_at(EXTENT, Which::Coverage) else {
        return;
    };
    let upright = Drawn::plain(110.0, (140.0, 30.0));
    let flat = present(&mut harness, &scene_of("Hl", upright));
    let turned = present(&mut harness, &scene_of("Hl", upright.turned(90.0)));

    let (fl, ft, fr, fb) = ink_extent(&flat).expect("the fallback draws the upright string");
    let (tl, tt, tr, tb) = ink_extent(&turned).expect("and the turned one");
    let (flat_width, flat_height) = (fr - fl, fb - ft);
    let (turned_width, turned_height) = (tr - tl, tb - tt);
    assert!(
        flat_width > flat_height,
        "the upright string is wider than it is tall: {flat_width} by {flat_height}"
    );
    assert!(
        (turned_width - flat_height).abs() <= 2 && (turned_height - flat_width).abs() <= 2,
        "a quarter turn transposes the extents on the fallback too: {flat_width} by {flat_height} \
         became {turned_width} by {turned_height}"
    );
}

/// A run promoted for its brush still draws when the brush is the thing the device cannot evaluate.
///
/// This is the case the promotion rule makes unavoidable: a ramp is the *reason* the run left the
/// atlas, so a fallback that skips what it cannot ramp skips the letters themselves. The colour is
/// the documented downgrade — one flat stand-in rather than a ramp — but the letters have to be
/// there, and they have to be letters rather than the line box filled in.
#[test]
fn a_gradient_run_still_draws_its_letters_on_the_fallback_rasteriser() {
    let Some(mut harness) = harness_at(EXTENT, Which::Coverage) else {
        return;
    };
    let scene = scene_of(
        "HHHHHHHHHHHH",
        Drawn::plain(16.0, (20.0, 20.0)).with_gradient(),
    );
    let (sprites, vectors) = counts(&scene);
    assert_eq!(sprites, 0, "a run painted with a ramp leaves the atlas");
    assert!(vectors > 0, "and is drawn as curves");

    let pixels = present(&mut harness, &scene);
    let ink = inked(&pixels);
    assert!(
        !ink.is_empty(),
        "the fallback draws the letters it cannot ramp, rather than skipping them"
    );
    let leftmost = ink.iter().map(|(x, _)| *x).min().expect("ink");
    let rightmost = ink.iter().map(|(x, _)| *x).max().expect("ink");
    assert!(
        rightmost - leftmost > 40,
        "a dozen letters cover a good stretch of the line: {leftmost} to {rightmost}"
    );
    // Letters, not a filled rectangle: the counters and the gaps between the stems are still empty.
    let covered = (leftmost..=rightmost)
        .filter(|x| ink.iter().any(|(inked, _)| inked == x))
        .count();
    assert!(
        covered < (rightmost - leftmost) as usize,
        "the run leaves blank columns between its letters rather than filling its line box"
    );
}

/// A frame drawn against the rectangles that changed is the frame drawn whole.
///
/// Outline runs are where this is easy to get wrong: the rectangle a turned run paints is not the
/// rectangle it occupied upright, the vector pass is planned from the same rectangles the scissor
/// is, and a pass region that fell short leaves the previous frame's letters standing beside the
/// new ones. Only a pixel comparison against a full repaint says so.
#[test]
fn a_scissored_frame_of_an_outline_run_is_the_frame_repainted_whole() {
    let Some((mut scissored, mut whole)) = twins(EXTENT, Which::Vello) else {
        return;
    };
    let ink_of = |scene: &Scene| {
        scene
            .primitives
            .vectors
            .iter()
            .map(|item| item.ink)
            .reduce(|union, ink| union.union(ink))
            .expect("an outline run reports ink")
    };

    let before = scene_of("Hl", Drawn::plain(100.0, (60.0, 60.0)));
    let first = present(&mut scissored, &before);
    assert!(!inked(&first).is_empty(), "the first frame draws letters");
    let _ = present(&mut whole, &before);

    // The same letters, turned. What changed is everything either frame's curves cover.
    let after = scene_of("Hl", Drawn::plain(100.0, (60.0, 60.0)).turned(25.0));
    let mut damage = DamageSet::new();
    damage.absorb(whole_pixels(ink_of(&before)));
    damage.absorb(whole_pixels(ink_of(&after)));
    assert!(
        !damage.is_full(),
        "a turned run damages part of the surface, or this case compares a repaint with itself"
    );

    let outcome = scissored.renderer.draw(&after, &damage);
    assert!(outcome.retires_damage(), "{outcome:?}");
    let partial = scissored
        .renderer
        .read_presented()
        .expect("a stand-in surface can be read back");
    let repainted = present(&mut whole, &after);
    assert_eq!(
        difference(EXTENT, &partial, &repainted),
        None,
        "a frame drawn against its damage is not the frame drawn whole"
    );
}

/// Draws through a renderer that may have nothing attached, and reads back what was presented.
fn present_unattached(renderer: &mut WgpuRenderer, scene: &Scene) -> Pixels {
    renderer.draw(scene, &DamageSet::full());
    renderer
        .read_presented()
        .expect("a stand-in surface can be read back")
}

/// A coordinate system directly under the viewport, holding `matrix`.
///
/// Named after a made-up owner, because a scene built by hand has no boxes to name one after.
fn space(scene: &mut Scene, owner: u64, matrix: zgui_geom::Matrix4) -> zgui_scene::SpatialId {
    let viewport = scene.spatial.viewport();
    let owner = zgui_scene::PropertyOwner::new(owner).expect("not the empty word");
    let own = zgui_scene::OwnSpace::of(Some(matrix), None, false);
    scene.spatial.space_of(viewport, owner, own)
}
