// DERIVED-FROM: the GPUI project, crates/gpui_wgpu/src/shaders.wgsl (Apache-2.0)
// The two stages of the polychrome sprite pipeline are adapted from that work, which is licensed
// under the Apache License, Version 2.0, and have been modified: the tile is addressed in atlas
// texels rather than normalised coordinates, the clip is a chain evaluated by a shared coverage
// function rather than four interpolated distances, the texels are premultiplied, and the rounded
// corner is a pair of elliptical semi-axes per corner rather than a scalar radius.

// Full-colour sprites: emoji, and decoded images.

@group(1) @binding(0) var<storage, read> sprites: array<ColorSprite>;

@vertex
fn vs_color_sprite(
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
fn fs_color_sprite(in: SpriteVarying) -> @location(0) vec4<f32> {
    let sprite = sprites[in.instance];
    // The level of detail comes from the texel-position derivatives, taken here because control
    // flow is still uniform: after the clip branch below they would be undefined. Never negative,
    // because magnification is the sampler's business rather than a level.
    let texel_dx = dpdx(in.texel);
    let texel_dy = dpdy(in.texel);
    let lod = 0.5 * log2(max(max(dot(texel_dx, texel_dx), dot(texel_dy, texel_dy)), 1.0));
    let clip = clip_coverage(device_position(in.position.xy), sprite.clip);
    if clip <= 0.0 {
        return vec4<f32>(0.0);
    }
    // The texels are premultiplied, which is what keeps a soft edge over a light background soft
    // instead of blooming: a half-covered edge texel contributes half its colour, not all of it.
    var texel = sample_atlas(in.texel, sprite.tile, lod);
    if (sprite.flags & GRAYSCALE) != 0u {
        let gray = color_brightness(straight_rgb(texel));
        texel = vec4<f32>(vec3<f32>(gray) * texel.a, texel.a);
    }
    // Coverage against the frame rather than the quad: a `cover` picture is cut to its box, a
    // letterboxed one keeps drawing only where it is, and the rounded corners follow the box in
    // both cases.
    let rounded = rect_coverage(in.local, sprite.frame, sprite.radii);
    return texel * sprite.opacity * rounded * clip;
}
