// DERIVED-FROM: the GPUI project, crates/gpui_wgpu/src/shaders_subpixel.wgsl (Apache-2.0)
// The dual-source blend formulation — colour on one output and per-channel coverage on the other,
// so the blend factor is the coverage itself — and the display-order swap are adapted from that
// work, which is licensed under the Apache License, Version 2.0, and have been modified: the
// coverage correction is shared with the single-channel pipeline and the clip is a chain evaluated
// by the shared coverage function.

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

struct SubpixelOutput {
    @location(0) @blend_src(0) color: vec4<f32>,
    @location(0) @blend_src(1) coverage: vec4<f32>,
}

@vertex
fn vs_subpixel_sprite(
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
fn fs_subpixel_sprite(in: SpriteVarying) -> SubpixelOutput {
    let clip = clip_coverage(device_position(in.position.xy), in.v_clip);
    let color = in.v_color;
    let straight = straight_rgb(color);

    var sample = sample_atlas(in.texel, in.v_tile, 0.0).rgb;
    if globals.text.z != 0.0 {
        // The display's subpixels run the other way round, so the coverage does too.
        sample = sample.bgr;
    }
    let coverage = correct_coverage3(sample, straight, globals.text.y) * clip * color.a;

    var out: SubpixelOutput;
    // The colour is written straight and the per-channel coverage is the blend factor, which is
    // why this pipeline writes no alpha at all and why it is meaningless against a destination
    // that is not opaque. A run landing in a target that is not opaque is emitted as a
    // single-channel sprite instead, before it ever reaches here.
    out.color = vec4<f32>(straight, 1.0);
    out.coverage = vec4<f32>(coverage, 1.0);
    return out;
}
