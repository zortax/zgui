// DERIVED-FROM: the GPUI project, crates/gpui_wgpu/src/shaders_subpixel.wgsl (Apache-2.0)
// The dual-source blend formulation — colour on one output and per-channel coverage on the other,
// so the blend factor is the coverage itself — and the display-order swap are adapted from that
// work, which is licensed under the Apache License, Version 2.0, and have been modified: the
// coverage correction is shared with the single-channel pipeline and the clip is a chain evaluated
// by the shared coverage function.

@group(1) @binding(0) var<storage, read> sprites: array<Sprite>;

struct SubpixelOutput {
    @location(0) @blend_src(0) color: vec4<f32>,
    @location(0) @blend_src(1) coverage: vec4<f32>,
}

@vertex
fn vs_subpixel_sprite(
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
fn fs_subpixel_sprite(in: SpriteVarying) -> SubpixelOutput {
    let sprite = sprites[in.instance];
    let clip = clip_coverage(device_position(in.position.xy), sprite.clip);
    let color = rgba_of(sprite.color);
    let straight = straight_rgb(color);

    var sample = sample_atlas(in.texel, sprite.tile, 0.0).rgb;
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
