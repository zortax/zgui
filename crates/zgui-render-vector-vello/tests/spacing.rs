//! Where ordinary text lands, measured off the surface it landed on.
//!
//! This is the other half of the promotion rule, and the half a component library is made of: an
//! upright run of one colour at a text size stays on the glyph atlas, and each of its letters is
//! drawn where shaping asked for it. Both halves are measured rather than inspected. A display list
//! carrying sprites proves the run was not promoted but not that a tile ever reached the surface —
//! a glyph whose texels were never written samples whatever the texture held before — and the
//! position a display list reports is the position the code under test computed, which is exactly
//! the number a placement bug gets wrong.
//!
//! So the letters are read back as pixels and segmented into their own columns of ink, and the
//! distance between one letter's ink and the next is compared with the distance between their
//! shaped advances. A run whose pen and phase are split from different numbers puts a letter very
//! nearly a whole pixel from where the shaper asked, crowding one neighbour and opening a gap to
//! the other; that shows up here as a residual approaching a pixel while the quantisation alone
//! stays near a fifth of one.

mod support;

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_css::StyleDraft;
use zgui_geom::{CssPx, DevicePx, Point, Rect, Size};
use zgui_layout::Paragraphs;
use zgui_layout::fragment::ParagraphId;
use zgui_layout::measure::MeasureContent;
use zgui_layout::tree::store::LayoutStore;
use zgui_paint::emit::text::{TextPlacement, emit};
use zgui_paint::{ContentCache, GlyphSource};
use zgui_render::Renderer;
use zgui_render_wgpu::Pixels;
use zgui_scene::{ClipId, Scene, SpatialId};
use zgui_text::{FontSource, ParagraphContent, ShapedGlyphs, StyledRun, TextMap};
use zgui_text_parley::{FontSystem, FontSystemOptions, Rasteriser, Shaper};
use zgui_text_style::{
    Direction, FamilyName, FontFamilyList, ParagraphStyle, TextAlign, TextStyle,
};

use support::{Harness, SIDE, Which, harness_at, present};

/// The extent every case here draws into.
const EXTENT: i32 = 512;

/// The face these cases are drawn in, which ships with the text engine's own tests.
const FACE: &str = "Noto Sans";

/// The glyph identifier of the face's blank glyph, which is allocated no tile.
const BLANK: u16 = 3;

/// Reads one of the faces shipped with the text engine's own tests.
fn face(file: &str) -> Vec<u8> {
    let path = format!(
        "{}/../zgui-text-parley/tests/fonts/{file}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|error| panic!("reading {path}: {error}"))
}

/// The font system holding the shipped face and nothing the machine happens to have.
fn fonts() -> Arc<FontSystem> {
    let system = Arc::new(FontSystem::new(FontSystemOptions::registered_only()));
    system
        .register(Arc::new(face("NotoSans-Regular.ttf")), None)
        .expect("the Latin face registers");
    system
}

/// What one run was drawn as, and what it drew.
struct Drawn {
    /// The surface it landed on.
    pixels: Pixels,
    /// Where shaping put each glyph that has ink, relative to the line box's left edge.
    shaped: Vec<f32>,
    /// How many sprites the display list carried.
    sprites: usize,
    /// How many filled curves it carried.
    vectors: usize,
}

/// Shapes, places, uploads and draws one run, and reads the surface back.
///
/// Every stage is the real one, and the tiles go to the renderer's own atlas rather than to a sink
/// that discards them, because a tile that was never uploaded is the difference between a display
/// list that looks right and a surface with the wrong pixels on it.
fn draw(harness: &mut Harness, text: &str, size: f32, origin: (f32, f32)) -> Drawn {
    let fonts = fonts();
    let mut paragraphs = Paragraphs::new(Shaper::new(Arc::clone(&fonts)));

    let mut map = TextMap::new();
    map.push(0..text.len(), 0, 0);
    let style = Arc::new(TextStyle {
        family: FontFamilyList::from_iter([FamilyName::Named(zgui_interned::Ident::new(FACE))]),
        size: CssPx(size),
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

    // Straight from the engine, before anything downstream has touched it.
    let mut shaped = Vec::new();
    paragraphs.visit_line(summary.key, 0, &mut |run| {
        shaped.extend(
            run.glyphs
                .iter()
                .filter(|glyph| glyph.glyph != BLANK)
                .map(|glyph| glyph.x),
        );
    });

    let mut store = LayoutStore::new(zgui_arena::DocumentId::FIRST);
    let paragraph = store.intern_paragraph(summary.key);

    let mut scene = Scene::new();
    scene.begin_frame(Size::new(EXTENT, EXTENT));
    let mut lowered = zgui_paint::lower::lower(&StyleDraft::initial().build(), 1.0);
    lowered.color = Color::srgb(0.0, 0.0, 0.0, 1.0);

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
            zgui_paint::emit::text::Inherited::default(),
            TextPlacement {
                line: Rect::new(
                    Point::new(DevicePx(origin.0), DevicePx(origin.1)),
                    Size::new(DevicePx(480.0), DevicePx(size * 1.4)),
                ),
                clip: ClipId::ROOT,
                transform: SpatialId::VIEWPORT,
                opaque_target: true,
                subpixel_capable: false,
                upright: true,
                scale: 1.0,
                ellipsis: None,
            },
        );
    }
    cache
        .flush(harness.renderer.texture_sink())
        .expect("the device accepts the tiles this frame allocated");

    scene.finish(&DamageSet::full());
    let sprites = scene.primitives.mono_sprites.len()
        + scene.primitives.subpixel_sprites.len()
        + scene.primitives.color_sprites.len();
    let vectors = scene.primitives.vectors.len();
    let pixels = present(harness, &scene);
    Drawn {
        pixels,
        shaped,
        sprites,
        vectors,
    }
}

/// The alpha-weighted centre of each unbroken column-run of ink on the surface.
///
/// Letters separated by their side bearings leave blank columns between them, so the runs are the
/// letters — and the centre of a letter's ink is a position no stage under test computed.
fn ink_centres(pixels: &Pixels) -> Vec<f32> {
    let mut centres = Vec::new();
    let mut current: Option<(f64, f64)> = None;
    for x in 0..EXTENT {
        let mut weight = 0.0;
        for y in 0..EXTENT {
            weight += f64::from(pixels.rgba(x, y)[3]);
        }
        if weight > 0.5 {
            let entry = current.get_or_insert((0.0, 0.0));
            entry.0 += weight;
            entry.1 += weight * (f64::from(x) + 0.5);
        } else if let Some((weight, moment)) = current.take() {
            centres.push((moment / weight) as f32);
        }
    }
    if let Some((weight, moment)) = current.take() {
        centres.push((moment / weight) as f32);
    }
    centres
}

/// How far each letter's ink is from where its own advance put it, relative to the first letter.
///
/// Taking every distance relative to the first letter is what makes the residual a property of the
/// placement rather than of the face: every letter here is the same letter, so its ink sits the same
/// distance from its origin, and whatever that distance is cancels.
fn residuals(centres: &[f32], shaped: &[f32]) -> Vec<f32> {
    centres
        .iter()
        .zip(shaped)
        .map(|(centre, x)| (centre - centres[0]) - (x - shaped[0]))
        .collect()
}

/// The root-mean-square of a set of residuals, and the worst of them.
fn spread(residuals: &[f32]) -> (f32, f32) {
    let mean =
        residuals.iter().map(|r| f64::from(r * r)).sum::<f64>() / residuals.len().max(1) as f64;
    (
        mean.sqrt() as f32,
        residuals.iter().fold(0.0f32, |worst, r| worst.max(r.abs())),
    )
}

/// An upright run of one colour at a text size draws from the atlas, and its letters land where
/// shaping asked for them.
///
/// The sizes are deliberately not whole numbers and neither is the line box's left edge: an advance
/// that lands on a whole pixel never exercises the split at all, and a line box at a whole pixel
/// hides a phase taken from the run-relative position rather than from the absolute one.
#[test]
fn an_ordinary_run_draws_from_the_atlas_where_shaping_asked() {
    let _ = SIDE;
    let Some(mut harness) = harness_at(EXTENT, Which::Vello) else {
        return;
    };

    // The same letter thirty times: identical ink, so a difference between two letters' positions
    // is a difference in where they were placed and nothing else.
    let text = "HHHHHHHHHHHHHHHHHHHHHHHHHHHHHH";
    for (size, left) in [(16.3f32, 7.3f32), (15.7, 0.0), (16.0, 3.5), (13.9, 11.25)] {
        let drawn = draw(&mut harness, text, size, (left, 40.0));
        assert_eq!(
            drawn.vectors, 0,
            "an upright run of one colour at {size} pixels is never filled as curves"
        );
        assert_eq!(
            drawn.sprites,
            text.len(),
            "every letter of it is a sprite reading a tile"
        );

        let centres = ink_centres(&drawn.pixels);
        assert_eq!(
            centres.len(),
            drawn.shaped.len(),
            "every letter drew its own column of ink at {size} pixels, separated from its \
             neighbours — and {} columns for {} letters means tiles that were never uploaded, or \
             letters drawn on top of one another",
            centres.len(),
            drawn.shaped.len()
        );

        let residuals = residuals(&centres, &drawn.shaped);
        let (rms, worst) = spread(&residuals);
        assert!(
            rms <= 0.36,
            "the letters at {size} pixels from {left} are {rms} px out of step on average: \
             {residuals:?}"
        );
        assert!(
            worst <= 0.6,
            "one letter at {size} pixels from {left} is {worst} px from where its advance put it, \
             which is the gap and the crowding a split pen and phase produce: {residuals:?}"
        );
    }
}
