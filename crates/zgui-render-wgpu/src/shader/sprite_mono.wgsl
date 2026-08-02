// DERIVED-FROM: the GPUI project, crates/gpui_wgpu/src/shaders.wgsl (Apache-2.0)
// The two stages of the monochrome sprite pipeline are adapted from that work, which is licensed
// under the Apache License, Version 2.0, and have been modified: the tile is addressed in atlas
// texels rather than normalised coordinates, the clip is a chain evaluated by a shared coverage
// function rather than four interpolated distances, and the tint is premultiplied sRGB rather than
// an HSLA quadruple converted in the vertex stage.

// Single-channel coverage sprites: glyphs, and shapes rasterised as alpha masks.

@group(1) @binding(0) var<storage, read> sprites: array<Sprite>;

@vertex
fn vs_mono_sprite(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32,
) -> SpriteVarying {
    let sprite = sprites[instance];
    let corner = unit_corner(vertex);
    let local = bounds_origin(sprite.bounds) + corner * bounds_size(sprite.bounds);
    var out: SpriteVarying;
    out.position = to_clip_position(local, sprite.transform);
    out.local = local;
    out.texel = tile_texel(corner, sprite.tile);
    out.instance = instance;
    return out;
}

@fragment
fn fs_mono_sprite(in: SpriteVarying) -> @location(0) vec4<f32> {
    let sprite = sprites[in.instance];
    let clip = clip_coverage(device_position(in.position.xy), sprite.clip);
    if clip <= 0.0 {
        return vec4<f32>(0.0);
    }
    let color = rgba_of(sprite.color);
    let sample = sample_atlas(in.texel).r;
    let coverage = correct_coverage(sample, straight_rgb(color), globals.text.x);
    return color * coverage * clip;
}
