// Converts one video frame's Y'CbCr planes to gamma-encoded R'G'B' in a full-target triangle.

struct Params {
    // One output channel per row: weights for Y', Cb and Cr, then an offset.
    red: vec4<f32>,
    green: vec4<f32>,
    blue: vec4<f32>,
    // xy: the visible fraction of the luma plane. z: 1 when Cr is the chroma plane's second
    // channel, 0 when it is a plane of its own.
    shape: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var bilinear: sampler;
@group(0) @binding(2) var luma: texture_2d<f32>;
@group(0) @binding(3) var cb: texture_2d<f32>;
@group(0) @binding(4) var cr: texture_2d<f32>;

struct Varyings {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> Varyings {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: Varyings;
    out.position = vec4<f32>(corner * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = corner * params.shape.xy;
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let y = textureSampleLevel(luma, bilinear, in.uv, 0.0).r;
    let blue_difference = textureSampleLevel(cb, bilinear, in.uv, 0.0);
    let red_difference = textureSampleLevel(cr, bilinear, in.uv, 0.0);
    let sample = vec4<f32>(
        y,
        blue_difference.r,
        select(red_difference.r, red_difference.g, params.shape.z > 0.5),
        1.0,
    );
    let rgb = vec3<f32>(dot(params.red, sample), dot(params.green, sample), dot(params.blue, sample));
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
