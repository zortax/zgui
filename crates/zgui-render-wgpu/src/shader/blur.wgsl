// DERIVED-FROM: the GPUI project, crates/gpui_wgpu/src/shaders.wgsl (Apache-2.0)
// The separable gaussian chain — a snapped 2:1 box downsample followed by two single-axis passes
// whose taps spread out rather than truncating when the radius outruns the tap budget — is adapted
// from that work, which is licensed under the Apache License, Version 2.0, and has been modified:
// the reference anchors its half-resolution grid and its pass geometry to the window's viewport,
// so pointing it at a sub-rectangle re-anchors the sampling lattice to a moving origin and the
// halo wobbles about a pixel per frame under animation. Here every pass takes the source and
// destination extents and the device-pixel scale of each as uniforms, and the lattice is anchored
// to the device origin, so it is fixed wherever the blurred content happens to sit.
//
// Licensed under the Apache License, Version 2.0.

struct BlurParams {
    // xy: the source texture's extent in texels; zw: the destination's.
    extents: vec4<f32>,
    // xy: this pass's direction, in source texels; z: the deviation, in source texels;
    // w: how many taps to take on each side of the centre.
    kernel: vec4<f32>,
    // x: how many texels of the source one device pixel covers; y: the same for the destination;
    // z: the spacing between taps, in source texels; w: non-zero when this pass halves resolution.
    sampling: vec4<f32>,
    // The part of the source that was actually written, in source texels: the two corners.
    //
    // Reads are clamped to it. Everything outside is either a texel this pass's region did not
    // cover or one a previous lease left behind, and a filter that read either would give a
    // different answer depending on how much of the frame was being redrawn — which is exactly
    // the property that has to hold whatever the damage set says.
    valid: vec4<f32>,
}

@group(0) @binding(0) var<uniform> blur: BlurParams;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var source_sampler: sampler;

const BLUR_PI: f32 = 3.141592653589793;

struct BlurVarying {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_blur(@builtin(vertex_index) vertex: u32) -> BlurVarying {
    // A strip over the whole destination; the scissor is what restricts it to the region being
    // filtered, so a blurred panel costs its own area rather than the window's.
    let corner = vec2<f32>(f32(vertex & 1u), 0.5 * f32(vertex & 2u));
    var out: BlurVarying;
    out.position = vec4<f32>(corner.x * 2.0 - 1.0, 1.0 - corner.y * 2.0, 0.0, 1.0);
    return out;
}

// The device pixel a destination texel's centre sits at.
fn blur_device_position(position: vec2<f32>) -> vec2<f32> {
    return (floor(position) + vec2<f32>(0.5)) / blur.sampling.y;
}

// Where a device position lands in the source, in source texels.
fn blur_source_texel(device: vec2<f32>) -> vec2<f32> {
    return device * blur.sampling.x;
}

// The source at a position in source texels, clamped to what was written.
fn blur_sample(texel: vec2<f32>) -> vec4<f32> {
    let low = blur.valid.xy + vec2<f32>(0.5);
    let high = max(blur.valid.zw - vec2<f32>(0.5), low);
    return textureSampleLevel(source, source_sampler, clamp(texel, low, high) / blur.extents.xy, 0.0);
}

@fragment
fn fs_blur_downsample(in: BlurVarying) -> @location(0) vec4<f32> {
    // A half-resolution texel covers two device pixels, so its centre lies exactly on the boundary
    // between the two source texels it averages, and one bilinear tap is that average. The grid is
    // anchored at the device origin rather than at the region's, which is what keeps the halo of a
    // blurred element still while the element moves by a fraction of a pixel.
    return blur_sample(blur_source_texel(blur_device_position(in.position.xy)));
}

@fragment
fn fs_blur_axis(in: BlurVarying) -> @location(0) vec4<f32> {
    let deviation = max(blur.kernel.z, 1e-4);
    let taps = i32(blur.kernel.w);
    let spacing = blur.sampling.z;
    let centre = blur_source_texel(blur_device_position(in.position.xy));
    let direction = blur.kernel.xy;

    var accumulated = vec4<f32>(0.0);
    var weight = 0.0;
    for (var tap = -taps; tap <= taps; tap = tap + 1) {
        let offset = f32(tap) * spacing;
        let contribution = gaussian_weight(offset, deviation);
        accumulated += blur_sample(centre + direction * offset) * contribution;
        weight += contribution;
    }
    return accumulated / max(weight, 1e-5);
}

fn gaussian_weight(distance: f32, deviation: f32) -> f32 {
    return exp(-(distance * distance) / (2.0 * deviation * deviation))
        / (sqrt(2.0 * BLUR_PI) * deviation);
}
