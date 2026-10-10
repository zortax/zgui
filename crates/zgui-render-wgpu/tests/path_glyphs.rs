//! The glyph lane draws each copy from the cell of its own device phase.

mod support;

use std::sync::Arc;

use zgui_atlas::{Atlas, AtlasKey, AtlasLimits, TextureKind};
use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::Size;
use zgui_scene::{MarkFlags, MarkItem, MarkPayload, PaintRef, Scene, SpriteTile};

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
