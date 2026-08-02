// DERIVED-FROM: the GPUI project, crates/gpui_wgpu/src/shaders.wgsl (Apache-2.0)
// The sprite instance layout, the unit-square expansion and the tile lookup are adapted from that
// work, which is licensed under the Apache License, Version 2.0, and have been modified: a tile is
// addressed in texels of the atlas it happens to live in rather than in a normalised rectangle
// carried per instance, and the clip is a chain evaluated by the shared coverage function.

// A single-channel or per-channel coverage sprite: a glyph, or a shape rasterised as a mask.
struct Sprite {
    order: u32,
    reserved: u32,
    bounds: Bounds,
    color: Rgba,
    tile: Tile,
    clip: u32,
    transform: u32,
}

// A full-colour sprite: an emoji, or a decoded image. Its texels are premultiplied.
struct ColorSprite {
    order: u32,
    flags: u32,
    bounds: Bounds,
    radii: Radii,
    tile: Tile,
    opacity: f32,
    clip: u32,
    transform: u32,
}

@group(2) @binding(0) var atlas: texture_2d<f32>;
@group(2) @binding(1) var atlas_sampler: sampler;

const GRAYSCALE: u32 = 1u;

struct SpriteVarying {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) texel: vec2<f32>,
    @location(2) @interpolate(flat) instance: u32,
}

// Where in the atlas a unit-square corner reads from, in texels.
//
// A sprite is drawn at its exact extent with no antialiasing slack: the coverage in the tile
// already carries the shape's edge, and inflating the quad would sample outside the tile.
//
// It stays in texels through the varying so that the atlas is read in the fragment stage alone:
// normalising it in the vertex stage would need the texture's extent there, and a texture bound to
// both stages is a texture every pipeline pays for in both.
fn tile_texel(corner: vec2<f32>, tile: Tile) -> vec2<f32> {
    let rect = tile_bounds(tile);
    return rect.xy + corner * rect.zw;
}

// The atlas, sampled at a position given in texels.
fn sample_atlas(texel: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(
        atlas,
        atlas_sampler,
        texel / vec2<f32>(textureDimensions(atlas)),
        0.0,
    );
}

// The straight colour a premultiplied tint stands for, which is what a glyph's contrast is judged
// against.
fn straight_rgb(color: vec4<f32>) -> vec3<f32> {
    if color.a <= 0.0 {
        return color.rgb;
    }
    return color.rgb / color.a;
}
