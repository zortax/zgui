// The copy from the composed target to the surface.
//
// It is a pure copy: no blend, no filtering, no sampler. `textureLoad` at the fragment's own
// coordinate reads exactly one texel, which is why the bind-group layout escapes the filtering
// restriction and why the copy cannot change a single value.
//
// The whole surface is copied every frame whatever was damaged, because every acquisition yields a
// fresh, wholly uninitialised surface texture: loading from one would cost a full clear first, so
// a partial copy would leave the rest black.

@group(0) @binding(0) var composed: texture_2d<f32>;

struct BlitVarying {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_blit(@builtin(vertex_index) vertex: u32) -> BlitVarying {
    // A triangle strip over the whole clip volume, which needs no vertex data at all.
    let corner = vec2<f32>(f32(vertex & 1u), 0.5 * f32(vertex & 2u));
    var out: BlitVarying;
    out.position = vec4<f32>(corner.x * 2.0 - 1.0, 1.0 - corner.y * 2.0, 0.0, 1.0);
    return out;
}

@fragment
fn fs_blit(in: BlitVarying) -> @location(0) vec4<f32> {
    return textureLoad(composed, vec2<i32>(in.position.xy), 0);
}

// The same copy, with the attachment's own encode cancelled in advance.
//
// This is the single place in the renderer where a shader converts between encodings, and it is
// legal exactly because the copy composites nothing: the attachment is about to apply an encode
// that this undoes, so the presented bytes are the composed target's bytes. It exists for surfaces
// that offer no unencoded format and cannot be viewed as one — which is every embedded and
// software GL driver, none of which report mutable surface view formats.
@fragment
fn fs_blit_undo_srgb(in: BlitVarying) -> @location(0) vec4<f32> {
    let texel = textureLoad(composed, vec2<i32>(in.position.xy), 0);
    return vec4<f32>(srgb_to_linear(texel.rgb), texel.a);
}

fn srgb_to_linear(color: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        srgb_channel_to_linear(color.r),
        srgb_channel_to_linear(color.g),
        srgb_channel_to_linear(color.b),
    );
}

fn srgb_channel_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        return value / 12.92;
    }
    return pow((value + 0.055) / 1.055, 2.4);
}
