// DERIVED-FROM: the GPUI project, crates/gpui_wgpu/src/shaders.wgsl (Apache-2.0)
// The Oklab conversion pair and the linear/encoded sRGB transfer functions are adapted from that
// work, which is licensed under the Apache License, Version 2.0, and have been modified: a ramp
// here carries any number of stops rather than exactly two, its stops arrive already expressed in
// the space the ramp is interpolated in, and the result is premultiplied gamma-encoded sRGB rather
// than that work's optionally-premultiplied output.

fn srgb_channel_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        return value / 12.92;
    }
    return pow((value + 0.055) / 1.055, 2.4);
}

fn linear_channel_to_srgb(value: f32) -> f32 {
    if value <= 0.0031308 {
        return value * 12.92;
    }
    return 1.055 * pow(value, 1.0 / 2.4) - 0.055;
}

fn srgb_to_linear(color: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        srgb_channel_to_linear(color.r),
        srgb_channel_to_linear(color.g),
        srgb_channel_to_linear(color.b),
    );
}

fn linear_to_srgb(color: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        linear_channel_to_srgb(color.r),
        linear_channel_to_srgb(color.g),
        linear_channel_to_srgb(color.b),
    );
}

// Reference: https://bottosson.github.io/posts/oklab/
fn oklab_to_linear_srgb(color: vec3<f32>) -> vec3<f32> {
    let l_ = color.x + 0.3963377774 * color.y + 0.2158037573 * color.z;
    let m_ = color.x - 0.1055613458 * color.y - 0.0638541728 * color.z;
    let s_ = color.x - 0.0894841775 * color.y - 1.2914855480 * color.z;

    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    return vec3<f32>(
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    );
}

// Where along a ramp a device-space point lies, before the ramp's extent is applied.
fn gradient_position(paint: Paint, point: vec2<f32>) -> f32 {
    if paint.gradient == 0u {
        let start = vec2<f32>(paint.geometry.x, paint.geometry.y);
        let end = vec2<f32>(paint.geometry.z, paint.geometry.w);
        let axis = end - start;
        let length_squared = dot(axis, axis);
        if length_squared <= 0.0 {
            return 0.0;
        }
        return dot(point - start, axis) / length_squared;
    }
    if paint.gradient == 1u {
        let center = vec2<f32>(paint.geometry.x, paint.geometry.y);
        let radii = vec2<f32>(max(paint.geometry.z, 1e-6), max(paint.geometry.w, 1e-6));
        return length((point - center) / radii);
    }
    // Conic: the sweep starts at twelve o'clock and runs clockwise, which is what CSS specifies
    // and what the y-down device space makes of `atan2`.
    let center = vec2<f32>(paint.geometry.x, paint.geometry.y);
    let delta = point - center;
    let angle = atan2(delta.x, -delta.y) - paint.geometry.w;
    return sdf_fmod(angle / (2.0 * M_PI) + 1.0, 1.0);
}

// The stop position a ramp is sampled at, once repetition or clamping has been applied.
fn gradient_extent(paint: Paint, raw: f32) -> f32 {
    if paint.flags == 1u {
        return fract(raw);
    }
    return saturate(raw);
}

// The colour of a ramp at `t`, as premultiplied gamma-encoded sRGB.
fn sample_ramp(paint: Paint, t: f32) -> vec4<f32> {
    let count = paint.stop_count;
    if count == 0u {
        return vec4<f32>(0.0);
    }
    var low = stops[paint.stop_start];
    if count == 1u || t <= low.offset {
        return decode_stop(paint.space, vector4_of(low.color));
    }
    let last = stops[paint.stop_start + count - 1u];
    if t >= last.offset {
        return decode_stop(paint.space, vector4_of(last.color));
    }
    var index = 1u;
    loop {
        if index >= count {
            break;
        }
        let high = stops[paint.stop_start + index];
        if t <= high.offset {
            let span = max(high.offset - low.offset, 1e-6);
            let mixed = mix(
                vector4_of(low.color),
                vector4_of(high.color),
                saturate((t - low.offset) / span),
            );
            return decode_stop(paint.space, mixed);
        }
        low = high;
        index += 1u;
    }
    return decode_stop(paint.space, vector4_of(low.color));
}

// An interpolated stop, back in premultiplied gamma-encoded sRGB.
//
// Stops arrive premultiplied in their own space, because CSS interpolates gradients with
// premultiplied alpha; the un-premultiply here is what stops a ramp to transparent turning black
// through its middle.
fn decode_stop(space: u32, value: vec4<f32>) -> vec4<f32> {
    let alpha = value.a;
    if space == SPACE_SRGB {
        return value;
    }
    if alpha <= 0.0 {
        return vec4<f32>(0.0);
    }
    let straight = value.rgb / alpha;
    var linear = straight;
    if space == SPACE_OKLAB {
        linear = oklab_to_linear_srgb(straight);
    }
    return vec4<f32>(linear_to_srgb(max(linear, vec3<f32>(0.0))) * alpha, alpha);
}

// What a paint reference paints at a point in the primitive's own space.
//
// `origin` is where that space has its origin in the space the paint's own geometry is written in.
// A paint states its geometry — a gradient line, a ramp's centre, an image's destination — in the
// coordinates it was resolved against, so a primitive that has since moved is sampled at the point
// it came *from*: subtracting the origin is what makes a ramp travel with the shape it fills
// rather than stay where the shape used to be. It is zero for a primitive drawn where its paint
// was resolved, which is every primitive of a frame painted from scratch.
//
// A solid paint never reads the stop storage at all, which is the whole reason the family travels
// in the instance beside the index.
fn paint_color(reference: PaintRef, point: vec2<f32>, origin: vec2<f32>) -> vec4<f32> {
    if reference.kind == PAINT_NONE {
        return vec4<f32>(0.0);
    }
    let paint = paints[reference.index];
    if reference.kind == PAINT_SOLID {
        return rgba_of(paint.color);
    }
    let anchored = point - origin;
    if reference.kind == PAINT_GRADIENT {
        return sample_ramp(paint, gradient_extent(paint, gradient_position(paint, anchored)));
    }
    // A sampled paint reaches the surface as a colour sprite rather than as a fill, so nothing
    // produces one here.
    return vec4<f32>(0.0);
}
