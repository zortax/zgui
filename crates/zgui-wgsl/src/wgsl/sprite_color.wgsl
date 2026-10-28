// DERIVED-FROM: the GPUI project, crates/gpui_wgpu/src/shaders.wgsl (Apache-2.0)
// The two stages of the polychrome sprite pipeline are adapted from that work, which is licensed
// under the Apache License, Version 2.0, and have been modified: the tile is addressed in atlas
// texels rather than normalised coordinates, the clip is a chain evaluated by a shared coverage
// function rather than four interpolated distances, the texels are premultiplied, and the rounded
// corner is a pair of elliptical semi-axes per corner rather than a scalar radius.

// Full-colour sprites: emoji, and decoded images.

@group(1) @binding(0) var sprites: texture_2d<u32>;

/// One `ColorSprite`, which the arena pads to 7 texels.
fn load_sprite(slot: u32) -> ColorSprite {
    let base = slot * 7u;
    let t0 = textureLoad(sprites, table_texel(base + 0u), 0);
    let t1 = textureLoad(sprites, table_texel(base + 1u), 0);
    let t2 = textureLoad(sprites, table_texel(base + 2u), 0);
    let t3 = textureLoad(sprites, table_texel(base + 3u), 0);
    let t4 = textureLoad(sprites, table_texel(base + 4u), 0);
    let t5 = textureLoad(sprites, table_texel(base + 5u), 0);
    let t6 = textureLoad(sprites, table_texel(base + 6u), 0);
    return ColorSprite(
        t0.x,
        t0.y,
        Bounds(bitcast<f32>(t0.z), bitcast<f32>(t0.w), bitcast<f32>(t1.x), bitcast<f32>(t1.y)),
        Bounds(bitcast<f32>(t1.z), bitcast<f32>(t1.w), bitcast<f32>(t2.x), bitcast<f32>(t2.y)),
        Radii(bitcast<f32>(t2.z), bitcast<f32>(t2.w), bitcast<f32>(t3.x), bitcast<f32>(t3.y), bitcast<f32>(t3.z), bitcast<f32>(t3.w), bitcast<f32>(t4.x), bitcast<f32>(t4.y)),
        Tile(t4.z, t4.w, TileRect(bitcast<i32>(t5.x), bitcast<i32>(t5.y), bitcast<i32>(t5.z), bitcast<i32>(t5.w))),
        bitcast<f32>(t6.x),
        t6.y,
        t6.z,
    );
}

@vertex
fn vs_color_sprite(
    @builtin(vertex_index) vertex: u32,
    @location(0) slot: u32,
    @location(1) shift: vec2<f32>,
) -> ColorSpriteVarying {
    let sprite = load_sprite(slot);
    let corner = unit_corner(vertex);
    let local = bounds_origin(sprite.bounds) + corner * bounds_size(sprite.bounds) + shift;
    var out: ColorSpriteVarying;
    out.position = to_clip_position(local, sprite.transform);
    out.local = corner * bounds_size(sprite.bounds);
    out.origin = bounds_origin(sprite.bounds);
    out.v_tile = tile_bounds(sprite.tile);
    out.texel = tile_texel(corner, out.v_tile);
    out.v_frame = bounds_of(sprite.frame);
    out.v_radii_lo = radii_lo(sprite.radii);
    out.v_radii_hi = radii_hi(sprite.radii);
    out.v_ids = vec2<u32>(sprite.clip, sprite.flags);
    out.v_opacity = sprite.opacity;
    return out;
}

@fragment
fn fs_color_sprite(in: ColorSpriteVarying) -> @location(0) vec4<f32> {
    // The level of detail comes from the texel-position derivatives, taken here because control
    // flow is still uniform: after the clip branch below they would be undefined. Never negative,
    // because magnification is the sampler's business rather than a level.
    let texel_dx = dpdx(in.texel);
    let texel_dy = dpdy(in.texel);
    let lod = 0.5 * log2(max(max(dot(texel_dx, texel_dx), dot(texel_dy, texel_dy)), 1.0));
    let clip = clip_coverage(device_position(in.position.xy), in.v_ids.x);
    if clip <= 0.0 {
        return vec4<f32>(0.0);
    }
    // The texels are premultiplied, which is what keeps a soft edge over a light background soft
    // instead of blooming: a half-covered edge texel contributes half its colour, not all of it.
    var texel = sample_atlas(in.texel, in.v_tile, lod);
    if (in.v_ids.y & GRAYSCALE) != 0u {
        let gray = color_brightness(straight_rgb(texel));
        texel = vec4<f32>(vec3<f32>(gray) * texel.a, texel.a);
    }
    // Coverage against the frame rather than the quad: a `cover` picture is cut to its box, a
    // letterboxed one keeps drawing only where it is, and the rounded corners follow the box in
    // both cases.
    let rounded = rect_coverage(
        in.local,
        bounds_from(bounds_from_vec4(in.v_frame), in.origin),
        radii_from_vec4(in.v_radii_lo, in.v_radii_hi),
        CORNER_ROUND,
    );
    return texel * in.v_opacity * rounded * clip;
}
