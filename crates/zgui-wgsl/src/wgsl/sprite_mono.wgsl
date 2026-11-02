// DERIVED-FROM: the GPUI project, crates/gpui_wgpu/src/shaders.wgsl (Apache-2.0)
// The two stages of the monochrome sprite pipeline are adapted from that work, which is licensed
// under the Apache License, Version 2.0, and have been modified: the tile is addressed in atlas
// texels rather than normalised coordinates, the clip is a chain evaluated by a shared coverage
// function rather than four interpolated distances, and the tint is premultiplied sRGB rather than
// an HSLA quadruple converted in the vertex stage.

// Single-channel coverage sprites: glyphs, and shapes rasterised as alpha masks.

@group(1) @binding(0) var sprites: texture_2d<u32>;

/// One `Sprite`, which the arena pads to 5 texels.
fn load_sprite(slot: u32) -> Sprite {
    let base = slot * 5u;
    let t0 = textureLoad(sprites, table_texel(base + 0u), 0);
    let t1 = textureLoad(sprites, table_texel(base + 1u), 0);
    let t2 = textureLoad(sprites, table_texel(base + 2u), 0);
    let t3 = textureLoad(sprites, table_texel(base + 3u), 0);
    let t4 = textureLoad(sprites, table_texel(base + 4u), 0);
    return Sprite(
        t0.x,
        t0.y,
        Bounds(bitcast<f32>(t0.z), bitcast<f32>(t0.w), bitcast<f32>(t1.x), bitcast<f32>(t1.y)),
        Rgba(bitcast<f32>(t1.z), bitcast<f32>(t1.w), bitcast<f32>(t2.x), bitcast<f32>(t2.y)),
        Tile(t2.z, t2.w, TileRect(bitcast<i32>(t3.x), bitcast<i32>(t3.y), bitcast<i32>(t3.z), bitcast<i32>(t3.w))),
        t4.x,
        t4.y,
    );
}

@vertex
fn vs_mono_sprite(
    @builtin(vertex_index) vertex: u32,
    @location(0) slot: u32,
    @location(1) shift: vec2<f32>,
) -> SpriteVarying {
    let sprite = load_sprite(slot);
    let corner = unit_corner(vertex);
    let local = bounds_origin(sprite.bounds) + corner * bounds_size(sprite.bounds) + shift;
    var out: SpriteVarying;
    out.position = to_clip_position(local, sprite.transform);
    out.v_tile = tile_bounds(sprite.tile);
    out.texel = tile_texel(corner, out.v_tile);
    out.v_color = rgba_of(sprite.color);
    out.v_clip = sprite.clip;
    return out;
}

@fragment
fn fs_mono_sprite(in: SpriteVarying) -> @location(0) vec4<f32> {
    let clip = clip_coverage(device_position(in.position.xy), in.v_clip);
    if clip <= 0.0 {
        return vec4<f32>(0.0);
    }
    let color = in.v_color;
    let sample = sample_atlas(in.texel, in.v_tile, 0.0).r;
    let coverage = correct_coverage(sample, straight_rgb(color), globals.text.x);
    return color * coverage * clip;
}
