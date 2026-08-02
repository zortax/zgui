//! One frame of the application, and what the display list said would be in it.

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::render::FrameOutcome;
use zgui::scene::Scene;
use zgui_render_wgpu::{Pixels, WgpuRenderer};

/// One drawing the display list held, and where it claimed its ink would land.
#[derive(Clone, Copy, Debug)]
pub struct Drawing {
    /// Its place in the painting order.
    pub order: usize,
    /// What it paints, in device pixels, as the display list measured it.
    pub ink: Rect<DevicePx, Device>,
    /// The clip chain it draws through.
    pub clip: u32,
    /// Whether it is filled, and whether it is stroked.
    pub painted: (bool, bool),
}

/// One filled rectangle the display list held.
///
/// The caret is one of these, and so is a border and a background — which is why the rectangle is
/// kept rather than a count: telling a caret from a field's own edge is a question about where and
/// how wide, and a fixture that counted quads would answer it for neither.
///
/// The rectangle is where the ink *lands*, which for a box under a transform is not the rectangle
/// the display list recorded. See [`placed`].
#[derive(Clone, Copy, Debug)]
pub struct Filled {
    /// Its place in the painting order.
    pub order: usize,
    /// Where it lands, in device pixels.
    pub bounds: Rect<DevicePx, Device>,
}

/// One glyph the display list held.
///
/// The tile is what makes this an assertion about *which* letters were drawn. A glyph is rasterised
/// once per face, size and glyph index and cached under that key, so two runs of the same string in
/// the same style read the same tiles in the same order — and two different strings do not.
///
/// The bounds are where the ink *lands*, which for text under a transform is not the rectangle the
/// display list recorded. See [`placed`].
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    /// Where it lands, in device pixels.
    pub bounds: Rect<DevicePx, Device>,
    /// Which texture of which pool holds its coverage, and which allocation in it.
    pub tile: (u32, u32),
}

/// What one of the application's frames produced.
pub struct Frame {
    /// The composed target, read back off the device.
    pub pixels: Pixels,
    /// Every vector item the display list held, in painting order.
    pub drawings: Vec<Drawing>,
    /// Every filled rectangle it held, in painting order.
    pub quads: Vec<Filled>,
    /// Every glyph it held, in painting order.
    pub glyphs: Vec<Glyph>,
    /// How many rasterisation passes the frame planned.
    pub passes: usize,
    /// How many vector items the damage cull dropped.
    pub culled: usize,
    /// How many passes the device reported having rasterised.
    pub rasterised: u32,
    /// Each pass's region, its item range and whether it composites per item.
    pub pass_regions: Vec<(Rect<i32, Device>, core::ops::Range<usize>, bool)>,
}

impl Frame {
    /// Reads back what `renderer` just drew, beside what `scene` said would be in it.
    pub fn record(renderer: &mut WgpuRenderer, scene: &Scene, outcome: &FrameOutcome) -> Self {
        let plan = scene.pass_plan();
        let drawings = scene
            .primitives
            .vectors
            .iter()
            .enumerate()
            .map(|(order, item)| Drawing {
                order,
                ink: item.ink,
                clip: item.clip.0,
                painted: (item.fill.is_some(), item.stroke.is_some()),
            })
            .collect();
        let quads = scene
            .primitives
            .quads
            .iter()
            .enumerate()
            .map(|(order, quad)| Filled {
                order,
                bounds: placed(scene, quad.bounds, quad.transform),
            })
            .collect();
        // All three sprite arrays, because which one a glyph lands in is the renderer's decision
        // about coverage and not a property of the text: a fixture that read only the monochrome
        // ones would see no letters at all on a machine that shapes them subpixel-antialiased.
        let mut glyphs: Vec<Glyph> = Vec::new();
        glyphs.extend(scene.primitives.mono_sprites.iter().map(|sprite| Glyph {
            bounds: placed(scene, sprite.bounds, sprite.transform),
            tile: (sprite.tile.texture, sprite.tile.tile),
        }));
        glyphs.extend(
            scene
                .primitives
                .subpixel_sprites
                .iter()
                .map(|sprite| Glyph {
                    bounds: placed(scene, sprite.bounds, sprite.transform),
                    tile: (sprite.tile.texture, sprite.tile.tile),
                }),
        );
        glyphs.extend(scene.primitives.color_sprites.iter().map(|sprite| Glyph {
            bounds: placed(scene, sprite.bounds, sprite.transform),
            tile: (sprite.tile.texture, sprite.tile.tile),
        }));

        Self {
            pixels: renderer
                .read_presented()
                .expect("these fixtures draw to a texture, which can be read back"),
            drawings,
            quads,
            glyphs,
            pass_regions: plan
                .passes
                .iter()
                .map(|pass| (pass.region, pass.items.clone(), pass.instanced))
                .collect(),
            passes: plan.passes.len(),
            culled: plan.culled,
            rasterised: outcome.stats().map_or(0, |stats| stats.vector_passes),
        }
    }
}

/// Where `bounds` lands once the transform it draws under has been applied.
///
/// A transform is applied when a box is *drawn*, not when it is laid out: the rectangle the display
/// list records is the one the box would have had untransformed, and the primitive carries the
/// matrix it reaches the screen through. A fixture that read the recorded rectangle would report a
/// translated strip as sitting exactly where it started — which is the one shape in which a
/// carousel that never moves and one that moves correctly are the same reading.
///
/// The four corners are put through the matrix and the box around them is taken, so a rotation
/// answers with the rectangle its ink actually covers.
fn placed(scene: &Scene, bounds: [f32; 4], transform: u32) -> Rect<DevicePx, Device> {
    let Some(matrix) = scene.spatial.resolve_at(transform) else {
        return rect_of(bounds);
    };
    let [x, y, width, height] = bounds;
    let corners = [
        matrix.transform_point(x, y, 0.0),
        matrix.transform_point(x + width, y, 0.0),
        matrix.transform_point(x, y + height, 0.0),
        matrix.transform_point(x + width, y + height, 0.0),
    ];
    let left = corners
        .iter()
        .map(|point| point[0])
        .fold(f32::MAX, f32::min);
    let right = corners
        .iter()
        .map(|point| point[0])
        .fold(f32::MIN, f32::max);
    let top = corners
        .iter()
        .map(|point| point[1])
        .fold(f32::MAX, f32::min);
    let bottom = corners
        .iter()
        .map(|point| point[1])
        .fold(f32::MIN, f32::max);
    rect_of([left, top, right - left, bottom - top])
}

/// One `[x, y, width, height]` as a rectangle.
fn rect_of(bounds: [f32; 4]) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(bounds[0]), DevicePx(bounds[1])),
        Size::new(DevicePx(bounds[2]), DevicePx(bounds[3])),
    )
}
