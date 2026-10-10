//! The glyph lane draws each copy from the cell of its own device phase.

mod support;

use std::sync::Arc;

use zgui_atlas::{Atlas, AtlasKey, AtlasLimits, TextureKind};
use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::Size;
use zgui_render_wgpu::{GroupPool, TargetScale};
use zgui_scene::{GroupBoundary, MarkFlags, MarkItem, MarkPayload, PaintRef, Scene, SpriteTile};

use support::{SIDE, plain_renderer, present, rect};

/// The coverage cell `phase` holds: a distinct level per phase.
fn level(phase: u32) -> u8 {
    (15 * (phase + 1)) as u8
}

/// Uploads a sheet of sixteen one-texel cells, cell `p` at `(p % 4, p / 4)` holding [`level`],
/// and returns the item's table and the packed texture.
fn sheet(renderer: &mut zgui_render_wgpu::WgpuRenderer) -> (Vec<[u32; 4]>, u32) {
    let mut atlas = Atlas::new(AtlasLimits::default());
    let tile = atlas
        .get_or_insert(
            AtlasKey::new(0x7E00_0000_0000_0001, TextureKind::Mono),
            Size::new(4, 4),
            || (0..16).map(level).collect::<Vec<u8>>(),
        )
        .expect("a fresh atlas has room");
    atlas
        .flush_uploads(renderer.atlas())
        .expect("the device accepts the upload");
    let (x, y) = (tile.bounds.origin.x as u32, tile.bounds.origin.y as u32);
    let table = (0..16)
        .map(|phase| {
            [
                (x + phase % 4) | ((y + phase / 4) << 16),
                1 | (1 << 16),
                0,
                0,
            ]
        })
        .collect();
    (table, SpriteTile::of(tile).texture)
}

/// One glyph per phase, the copy of phase `(px, py)` anchored at `(10 + 20 px + px / 4 + 0.01,
/// 10 + 20 py + py / 4 + 0.01)`, so it lands on pixel `(10 + 20 px, 10 + 20 py)`.
fn payload(table: &[[u32; 4]]) -> MarkPayload {
    let mut glyphs = table.to_vec();
    for phase in 0..16u32 {
        let (px, py) = ((phase % 4) as f32, (phase / 4) as f32);
        let x = 10.0 + 20.0 * px + px / 4.0 + 0.01;
        let y = 10.0 + 20.0 * py + py / 4.0 + 0.01;
        let offset = glyphs.len() as u32;
        glyphs.push([x.to_bits(), y.to_bits(), 0, offset]);
    }
    MarkPayload {
        glyphs,
        ..MarkPayload::default()
    }
}

/// Draws the glyphs of [`payload`] in white, through the union bin when `union`.
fn draw(union: bool) -> Option<zgui_render_wgpu::Pixels> {
    let mut renderer = plain_renderer()?;
    let (table, texture) = sheet(&mut renderer);
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let white = PaintRef::solid(scene.paints.solid(Color::srgb_u8(255, 255, 255, 255)));
    let payload = payload(&table);
    let mut item = MarkItem::new(rect(8.0, 8.0, 80.0, 80.0), white, payload.counts());
    item.tiles = table.len() as u32;
    item.texture = texture;
    if union {
        item.flags |= MarkFlags::UNION;
    }
    scene.push_marks(item, Arc::new(payload));
    scene.finish(&DamageSet::full());
    Some(present(&mut renderer, &scene))
}

#[test]
fn the_glyph_lane_draws_the_cell_of_the_device_phase() {
    for union in [false, true] {
        let Some(pixels) = draw(union) else {
            return;
        };
        for phase in 0..16u32 {
            let (px, py) = ((phase % 4) as i32, (phase / 4) as i32);
            let (x, y) = (10 + 20 * px, 10 + 20 * py);
            let alpha = pixels.rgba(x, y)[3];
            assert!(
                alpha.abs_diff(level(phase)) <= 1,
                "phase {phase} reads {alpha} at ({x}, {y}), union {union}"
            );
            for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
                assert_eq!(
                    pixels.rgba(x + dx, y + dy)[3],
                    0,
                    "a one-texel cell covers one pixel"
                );
            }
        }
    }
}

/// The fractions an anchor is placed at: every 32nd of a pixel, and a 256th either side of each
/// boundary between two phases.
fn fractions() -> Vec<f32> {
    let mut fractions: Vec<f32> = (0..32).map(|step| step as f32 / 32.0).collect();
    for boundary in [0.125f32, 0.375, 0.625, 0.875] {
        fractions.push(boundary - 1.0 / 256.0);
        fractions.push(boundary + 1.0 / 256.0);
    }
    fractions
}

#[test]
fn the_drawn_phase_lies_within_an_eighth_of_the_anchor() {
    // Each copy lights one pixel with the level of its phase, so the readback names the pixel and
    // the phase the shader chose on each axis.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let (table, texture) = sheet(&mut renderer);
    let fractions = fractions();
    let base = |index: usize| (10 + 12 * (index % 9), 10 + 12 * (index / 9));
    let anchors: Vec<[f32; 2]> = (0..fractions.len())
        .map(|index| {
            let (x, y) = base(index);
            let fy = fractions[(index * 7 + 3) % fractions.len()];
            [x as f32 + fractions[index], y as f32 + fy]
        })
        .collect();
    let mut glyphs = table.clone();
    for anchor in &anchors {
        let offset = glyphs.len() as u32;
        glyphs.push([anchor[0].to_bits(), anchor[1].to_bits(), 0, offset]);
    }
    let payload = MarkPayload {
        glyphs,
        ..MarkPayload::default()
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let white = PaintRef::solid(scene.paints.solid(Color::srgb_u8(255, 255, 255, 255)));
    let mut item = MarkItem::new(rect(0.0, 0.0, 128.0, 128.0), white, payload.counts());
    item.tiles = table.len() as u32;
    item.texture = texture;
    scene.push_marks(item, Arc::new(payload));
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    for (index, anchor) in anchors.iter().enumerate() {
        let (x, y) = base(index);
        let lit: Vec<(i32, i32, u8)> = (-1..3)
            .flat_map(|dy| (-1..3).map(move |dx| (x as i32 + dx, y as i32 + dy)))
            .filter_map(|(px, py)| {
                let alpha = pixels.rgba(px, py)[3];
                (alpha > 0).then_some((px, py, alpha))
            })
            .collect();
        let [(px, py, alpha)] = lit.as_slice() else {
            panic!("the copy at {anchor:?} lit {lit:?}");
        };
        let phase = (f32::from(*alpha) / 15.0).round() as i32 - 1;
        assert!((0..16).contains(&phase), "level {alpha} at {anchor:?}");
        let drawn = [
            *px as f32 + (phase % 4) as f32 / 4.0,
            *py as f32 + (phase / 4) as f32 / 4.0,
        ];
        for axis in 0..2 {
            assert!(
                (drawn[axis] - anchor[axis]).abs() <= 0.125,
                "the copy at {anchor:?} is drawn at {drawn:?}"
            );
        }
    }
}

/// Uploads one solid cell of `side` texels square, and returns a table that names it at every
/// phase and the packed texture.
fn solid(renderer: &mut zgui_render_wgpu::WgpuRenderer, side: u32) -> (Vec<[u32; 4]>, u32) {
    let mut atlas = Atlas::new(AtlasLimits::default());
    let tile = atlas
        .get_or_insert(
            AtlasKey::new(0x7E00_0000_0000_0002, TextureKind::Mono),
            Size::new(side as i32, side as i32),
            || vec![255u8; (side * side) as usize],
        )
        .expect("a fresh atlas has room");
    atlas
        .flush_uploads(renderer.atlas())
        .expect("the device accepts the upload");
    let (x, y) = (tile.bounds.origin.x as u32, tile.bounds.origin.y as u32);
    let table = vec![[x | (y << 16), side | (side << 16), 0, 0]; 16];
    (table, SpriteTile::of(tile).texture)
}

/// Draws a white cell of three pixels square from pixel `(10, 10)` inside a group, and the sum of
/// alpha around it, with the group held at half resolution when `half`.
fn grouped_ink(half: bool) -> Option<u32> {
    let mut renderer = plain_renderer()?;
    let (table, texture) = solid(&mut renderer, 3);
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let boundary = GroupBoundary::start(
        rect(0.0, 0.0, 32.0, 32.0),
        1.0,
        zgui_scene::peniko::BlendMode::default(),
        Default::default(),
    );
    scene.push_group(boundary.clone());
    let white = PaintRef::solid(scene.paints.solid(Color::srgb_u8(255, 255, 255, 255)));
    let mut glyphs = table.clone();
    let offset = glyphs.len() as u32;
    glyphs.push([10.01f32.to_bits(), 10.01f32.to_bits(), 0, offset]);
    let payload = MarkPayload {
        glyphs,
        ..MarkPayload::default()
    };
    let mut item = MarkItem::new(rect(8.0, 8.0, 8.0, 8.0), white, payload.counts());
    item.tiles = table.len() as u32;
    item.texture = texture;
    scene.push_marks(item, Arc::new(payload));
    scene.push_group(boundary.end());
    scene.finish(&DamageSet::full());
    if half {
        // One half-resolution target's worth of budget, and not one texel more.
        let allocated: Size<i32, zgui_geom::Device> = Size::new(256, 256);
        let extent = TargetScale::Half.extent(allocated);
        renderer.set_group_budget(
            u64::from(GroupPool::FORMAT.block_copy_size(None).unwrap_or(8))
                * extent.width as u64
                * extent.height as u64,
        );
    }
    let pixels = present(&mut renderer, &scene);
    assert_eq!(renderer.groups().degraded(), u32::from(half));
    let mut ink = 0;
    for y in 4..20 {
        for x in 4..20 {
            ink += u32::from(pixels.rgba(x, y)[3]);
        }
    }
    Some(ink)
}

#[test]
fn a_half_resolution_glyph_ending_on_an_odd_pixel_keeps_its_last_row_and_column() {
    // The cell covers pixels 10 to 12, and its last row and column are the first half of a
    // half-resolution texel.
    let Some(full) = grouped_ink(false) else {
        return;
    };
    let Some(half) = grouped_ink(true) else {
        return;
    };
    assert_eq!(full, 9 * 255);
    assert!(
        half.abs_diff(full) <= full / 10,
        "at half resolution the cell paints {half} of {full}"
    );
}
