// What an application's shader is handed, and the helpers it is given.
//
// An effect writes one function. Everything around it — the vertex stage, the clip, the blend
// state and the bind groups — belongs to the framework, so an effect draws inside its own box and
// leaves the rest of the frame alone.
//
// Every value an effect reads arrives as a function argument. Nothing here refers forward to
// anything the epilogue declares, so a translation unit assembled from these pieces reads in
// declaration order from top to bottom.
//
// Nothing here reads a binding either. A rectangle effect is assembled beside the shared
// vocabulary and a filter effect is not — a filter binds the content it reads at group zero, where
// that vocabulary binds the frame's own tables — so anything this file needed from there would be
// a helper that works in one mode and not the other.

// Where a fragment is, and when.
struct ShaderInput {
    // The position in the element's box, from zero to one on each axis.
    uv: vec2<f32>,
    // The position in the element's box, in device pixels.
    local: vec2<f32>,
    // The element's box, in device pixels.
    size: vec2<f32>,
    // Device pixels per CSS pixel.
    scale: f32,
    // Seconds since the document started.
    time: f32,
    // Seconds the previous frame took.
    delta: f32,
    // The pointer, in the same coordinates as `local`.
    pointer: vec2<f32>,
    // One while the pointer is over the element, zero while it is elsewhere.
    hovered: f32,
}

// Where the content a filter reads is, and how much of it may be read.
//
// A filter is handed this rather than reaching for it, because the texture holding the content is
// declared beside the stage that calls the filter and nothing here refers forward to it.
struct FilterSource {
    // The device position the element's own origin sits at.
    origin: vec2<f32>,
    // Texels of the source per device pixel.
    scale: f32,
    // The source's extent, in texels.
    extent: vec2<f32>,
    // The lowest and highest texel that may be read.
    low: vec2<f32>,
    high: vec2<f32>,
}

// The content beneath, read at `point` in the element's own device pixels.
//
// Reads are clamped to the region the step is allowed to sample, which is its own box grown by the
// reach the effect declared. Everything outside that is either a texel this pass did not cover or
// one a previous lease left behind, and a filter that read either would give a different answer
// depending on how much of the frame was being redrawn.
fn source_at(
    beneath: texture_2d<f32>,
    beneath_sampler: sampler,
    region: FilterSource,
    point: vec2<f32>,
) -> vec4<f32> {
    let texel = (region.origin + point) * region.scale;
    let low = region.low + vec2<f32>(0.5);
    let clamped = clamp(texel, low, max(region.high - vec2<f32>(0.5), low));
    return textureSampleLevel(beneath, beneath_sampler, clamped / region.extent, 0.0);
}

// A colour scaled by its own alpha, which is the form everything composites in.
//
// A shader returning straight colours makes a half-covered edge contribute a full-intensity
// colour, which is the bright fringe seen around anything with a soft edge.
fn premultiplied(color: vec3<f32>, alpha: f32) -> vec4<f32> {
    return vec4<f32>(color * alpha, alpha);
}

// Coverage of one device pixel by a shape whose signed distance is `distance`, negative inside.
//
// Half a pixel is the largest distance between a pixel's centre and an edge that covers it, which
// is why the band is that wide and no wider.
fn coverage_of(distance: f32) -> f32 {
    return saturate(0.5 - distance);
}

// Signed distance from `point` to a superellipse with the given semi-axes and exponent, negative
// inside. An exponent of two is an ellipse; four is close to the shape a rounded rectangle takes
// when its corners are smoothed.
fn superellipse_sdf(point: vec2<f32>, radii: vec2<f32>, exponent: f32) -> f32 {
    let r = max(radii, vec2<f32>(1.0e-4));
    let n = max(exponent, 1.0e-4);
    let normalised = pow(abs(point) / r, vec2<f32>(n));
    let value = pow(max(normalised.x + normalised.y, 1.0e-12), 1.0 / n);
    // A first-order distance: the implicit value over the magnitude of its own gradient.
    let gradient = max(length(normalised / max(abs(point), vec2<f32>(1.0e-4))), 1.0e-4);
    return (value - 1.0) / gradient;
}

// An sRGB colour from a hue in turns, a saturation and a lightness, each from zero to one.
fn hsl(hue: f32, saturation: f32, lightness: f32) -> vec3<f32> {
    let h = fract(hue) * 6.0;
    let c = (1.0 - abs(2.0 * lightness - 1.0)) * saturation;
    // A modulus with the sign of its left operand, written here because this file reads nothing
    // from the shared vocabulary.
    let wrapped = h - 2.0 * trunc(h / 2.0);
    let x = c * (1.0 - abs(wrapped - 1.0));
    var rgb = vec3<f32>(0.0);
    if h < 1.0 {
        rgb = vec3<f32>(c, x, 0.0);
    } else if h < 2.0 {
        rgb = vec3<f32>(x, c, 0.0);
    } else if h < 3.0 {
        rgb = vec3<f32>(0.0, c, x);
    } else if h < 4.0 {
        rgb = vec3<f32>(0.0, x, c);
    } else if h < 5.0 {
        rgb = vec3<f32>(x, 0.0, c);
    } else {
        rgb = vec3<f32>(c, 0.0, x);
    }
    return rgb + vec3<f32>(lightness - 0.5 * c);
}
