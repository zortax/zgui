//! Cached raster content, and the premultiplication that decides whether it has a halo.
//!
//! Every consumer blends with a premultiplied factor, so an edge texel of a soft-edged image has
//! to arrive with its colour already scaled by its alpha. Straight bytes uploaded verbatim make a
//! half-covered edge contribute its colour at full intensity — the bloom and dark halo seen around
//! avatars, logos and emoji — and no invariant test can see it. This is the pixel that can.

mod support;

use zgui_atlas::{Atlas, AtlasKey, AtlasLimits, AtlasTile, TextureKind};
use zgui_bits::DamageSet;
use zgui_geom::Size;
use zgui_render::Renderer as _;
use zgui_scene::{ColorSprite, MonoSprite, Quad, Scene, SubpixelSprite};

use support::{SIDE, opaque, plain_renderer, present, rect};

/// The tile's side, in texels.
const TILE: i32 = 16;

/// A soft-edged red square: opaque on the left, transparent on the right.
///
/// `premultiplied` decides whether the colour channels are scaled by the alpha, which is the whole
/// variable under test.
fn ramp(premultiplied: bool) -> Vec<u8> {
    let mut bytes = Vec::with_capacity((TILE * TILE * 4) as usize);
    for _ in 0..TILE {
        for x in 0..TILE {
            let alpha = (255 * x / (TILE - 1)) as u8;
            let alpha = 255 - alpha;
            let red = if premultiplied { alpha } else { 255 };
            bytes.extend_from_slice(&[red, 0, 0, alpha]);
        }
    }
    bytes
}

/// A single-channel coverage ramp, opaque on the left and empty on the right.
fn coverage() -> Vec<u8> {
    let mut bytes = Vec::with_capacity((TILE * TILE) as usize);
    for _ in 0..TILE {
        for x in 0..TILE {
            bytes.push(255 - (255 * x / (TILE - 1)) as u8);
        }
    }
    bytes
}

/// Uploads one tile of `kind` under `key` and hands back where it landed.
fn upload(
    renderer: &mut zgui_render_wgpu::WgpuRenderer,
    key: u64,
    kind: TextureKind,
    bytes: Vec<u8>,
) -> AtlasTile {
    let mut atlas = Atlas::new(AtlasLimits::default());
    let tile = atlas
        .get_or_insert(AtlasKey::new(key, kind), Size::new(TILE, TILE), || bytes)
        .expect("one small tile fits in a fresh atlas");
    atlas
        .flush_uploads(renderer.atlas())
        .expect("the device accepts the upload");
    tile
}

/// Draws `tile` as a full-colour sprite over a mid-grey field and reads the result back.
fn over_grey(renderer: &mut zgui_render_wgpu::WgpuRenderer, tile: AtlasTile) -> Vec<[u8; 4]> {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let grey = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(128, 128, 128)));
    scene.push_quad(Quad::filled(rect(0.0, 0.0, SIDE as f32, SIDE as f32), grey));
    scene.push_color_sprite(ColorSprite::new(
        rect(0.0, 0.0, TILE as f32, TILE as f32),
        tile,
    ));
    scene.finish(&DamageSet::full());
    let pixels = present(renderer, &scene);
    (0..TILE).map(|x| pixels.rgba(x, TILE / 2)).collect()
}

/// A large upload's staging leaves with its frame instead of staying warm.
///
/// Ordinary uploads round up to a power of two and wait mapped for reuse, which is right for a
/// steady trickle of glyphs and wrong for one multi-megabyte image: rounded up and retained, one
/// 1024-square picture would hold megabytes of mapped memory for two seconds after it went by.
#[test]
fn an_oversized_upload_leaves_no_staging_behind() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let side = 1024i32;
    let mut atlas = Atlas::new(AtlasLimits::default());
    for handle in 0..2u64 {
        atlas
            .get_or_insert(
                AtlasKey::new(handle, TextureKind::Image),
                Size::new(side, side),
                || vec![9u8; (side * side * 4) as usize],
            )
            .expect("a fresh atlas has room");
        atlas
            .flush_uploads(renderer.atlas())
            .expect("the device accepts the upload");
        assert_eq!(
            renderer.atlas().staging_bytes(),
            0,
            "a four-megabyte transfer keeps no staging chunk warm"
        );
    }
}

#[test]
fn a_premultiplied_tile_composites_its_soft_edge_without_blooming() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let tile = upload(&mut renderer, 1, TextureKind::Color, ramp(true));
    let row = over_grey(&mut renderer, tile);

    assert_eq!(row[0], [255, 0, 0, 255], "the opaque end is the tile's red");
    assert_eq!(
        row[(TILE - 1) as usize],
        [128, 128, 128, 255],
        "the transparent end is the field beneath"
    );
    // Eight texels in the alpha is 119/255, so the tile contributes 0.467 of its red and the
    // field contributes the remaining 0.533 of itself: red 187, green and blue 68. Uploaded
    // straight, the same texel would contribute its red at full intensity and clamp at 255.
    let half = row[(TILE / 2) as usize];
    assert!(
        (185..=189).contains(&half[0]),
        "half coverage contributes half its red, not all of it: {half:?}"
    );
    assert!(
        (66..=70).contains(&half[1]),
        "the field shows through in proportion: {half:?}"
    );
}

#[test]
fn the_same_tile_uploaded_straight_blooms_at_the_edge() {
    // The counterfactual, so the assertion above is about premultiplication rather than about
    // whatever the shader happens to do. Straight bytes make every partly covered texel contribute
    // its colour at full intensity, which is brighter everywhere the tile is not opaque.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let premultiplied = upload(&mut renderer, 1, TextureKind::Color, ramp(true));
    let correct = over_grey(&mut renderer, premultiplied);

    let straight = upload(&mut renderer, 2, TextureKind::Color, ramp(false));
    let bloomed = over_grey(&mut renderer, straight);

    let midpoint = (TILE / 2) as usize;
    assert!(
        bloomed[midpoint][0] > correct[midpoint][0] + 40,
        "straight bytes bloom: {:?} against {:?}",
        bloomed[midpoint],
        correct[midpoint]
    );
}

#[test]
fn a_coverage_tile_is_tinted_by_the_colour_it_is_drawn_with() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let tile = upload(&mut renderer, 1, TextureKind::Mono, coverage());

    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let white = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(255, 255, 255)));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        white,
    ));
    scene.push_mono_sprite(MonoSprite::new(
        rect(0.0, 0.0, TILE as f32, TILE as f32),
        tile,
        opaque(0, 0, 255),
    ));
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    let row: Vec<[u8; 4]> = (0..TILE).map(|x| pixels.rgba(x, TILE / 2)).collect();
    assert_eq!(row[0], [0, 0, 255, 255], "full coverage is the tint itself");
    assert_eq!(
        row[(TILE - 1) as usize],
        [255, 255, 255, 255],
        "no coverage leaves the field beneath"
    );
    for pair in row.windows(2) {
        assert!(
            pair[0][2] <= pair[1][2] && pair[0][0] <= pair[1][0],
            "coverage falls off monotonically: {row:?}"
        );
    }
}

#[test]
fn a_sprite_whose_texture_was_never_uploaded_draws_nothing_rather_than_a_strangers_pixels() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let grey = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(128, 128, 128)));
    scene.push_quad(Quad::filled(rect(0.0, 0.0, SIDE as f32, SIDE as f32), grey));
    scene.push_color_sprite(ColorSprite::new(
        rect(0.0, 0.0, TILE as f32, TILE as f32),
        AtlasTile {
            texture: zgui_atlas::TextureId::new(TextureKind::Color, 3),
            tile: zgui_atlas::TileId(0),
            bounds: zgui_geom::Rect::new(zgui_geom::Point::new(0, 0), Size::new(TILE, TILE)),
        },
    ));
    scene.finish(&DamageSet::full());

    let pixels = present(&mut renderer, &scene);
    assert_eq!(
        pixels.rgba(4, 4),
        [128, 128, 128, 255],
        "an unbacked sprite is skipped, not drawn against another texture"
    );
}

#[test]
fn per_channel_coverage_puts_a_fringe_on_a_stroke_that_single_channel_coverage_does_not() {
    // Only where the device can blend against a second colour output. Where it cannot, the
    // pipeline is never created and text is emitted as single-channel coverage instead — which is
    // a fallback rather than a device that draws no text, and is what this asserts there.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let subpixel_available = renderer.capabilities().subpixel_text;

    let tile = upload(&mut renderer, 3, TextureKind::Subpixel, subpixel_ramp());
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let white = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(255, 255, 255)));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        white,
    ));
    scene.push_subpixel_sprite(SubpixelSprite::new(
        rect(0.0, 0.0, TILE as f32, TILE as f32),
        tile,
        opaque(0, 0, 0),
    ));
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    let sample = pixels.rgba(TILE / 2, TILE / 2);
    if subpixel_available {
        assert!(
            sample[0] != sample[2],
            "each channel carries its own coverage: {sample:?}"
        );
    } else {
        assert_eq!(
            sample,
            [255, 255, 255, 255],
            "with no dual-source blending the pipeline does not exist and nothing is drawn"
        );
    }
}

/// A tile whose three channels carry different coverage, which is what per-channel text is.
fn subpixel_ramp() -> Vec<u8> {
    let mut bytes = Vec::with_capacity((TILE * TILE * 4) as usize);
    for _ in 0..TILE {
        for x in 0..TILE {
            let base = (255 * x / (TILE - 1)) as u8;
            bytes.extend_from_slice(&[base, base.saturating_add(40), base.saturating_add(80), 255]);
        }
    }
    bytes
}
