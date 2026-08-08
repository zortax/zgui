// A rectangle showing a texture the renderer did not draw: a video frame, a screen capture.
//
// There is one of these per source in a frame rather than thousands, so it binds its own texture
// and its own block instead of being instanced out of a buffer. What it shares with everything
// else is the clip: the same chain, evaluated by the same function, so a video inside a rounded,
// scrolled card is clipped exactly as the card's own background is.

struct ExternalParams {
    // The quad, in device pixels: origin then extent.
    bounds: vec4<f32>,
    // x: the clip chain; y: the transform; z: the multiplier on the texture's alpha;
    // w: non-zero when the texture's colours are already scaled by its alpha.
    control: vec4<f32>,
}

@group(1) @binding(0) var<uniform> foreign: ExternalParams;
@group(1) @binding(1) var external_texture: texture_2d<f32>;
@group(1) @binding(2) var external_sampler: sampler;

struct ExternalVarying {
    @builtin(position) position: vec4<f32>,
    // `device` is a reserved word in Metal, and member names reach the MSL output verbatim.
    @location(0) device_point: vec2<f32>,
    @location(1) uv: vec2<f32>,
}

@vertex
fn vs_external(@builtin(vertex_index) vertex: u32) -> ExternalVarying {
    let corner = unit_corner(vertex);
    let device = foreign.bounds.xy + corner * foreign.bounds.zw;
    var out: ExternalVarying;
    out.position = to_clip_position(device, u32(foreign.control.y));
    out.device_point = device;
    out.uv = corner;
    return out;
}

@fragment
fn fs_external(in: ExternalVarying) -> @location(0) vec4<f32> {
    let coverage = clip_coverage(in.device_point, u32(foreign.control.x));
    if coverage <= 0.0 {
        return vec4<f32>(0.0);
    }
    var texel = textureSampleLevel(external_texture, external_sampler, in.uv, 0.0);
    if foreign.control.w == 0.0 {
        // Everything composites premultiplied. Blending straight bytes as though they were
        // premultiplied makes a half-covered edge contribute a full-intensity colour, which is the
        // bright fringe seen around anything with a soft edge.
        texel = vec4<f32>(texel.rgb * texel.a, texel.a);
    }
    return texel * coverage * foreign.control.z;
}
