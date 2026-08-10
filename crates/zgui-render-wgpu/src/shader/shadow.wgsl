// DERIVED-FROM: the GPUI project, crates/gpui_wgpu/src/shaders.wgsl (Apache-2.0)
// The analytic blurred rounded rectangle — the `erf` approximation, the closed-form horizontal
// integral, the four-sample vertical accumulation and the inset complement — is adapted from that
// work, which is licensed under the Apache License, Version 2.0, and has been modified: the
// scanline's horizontal extent is the ellipse equation rather than the circle's, so a corner with
// two different semi-axes blurs correctly, and it reduces to that work's exact expression when the
// two are equal.

struct Shadow {
    order: u32,
    blur: f32,
    bounds: Bounds,
    radii: Radii,
    element_bounds: Bounds,
    element_radii: Radii,
    color: Rgba,
    clip: u32,
    transform: u32,
    inset: u32,
    reserved: u32,
}

@group(1) @binding(0) var<storage, read> shadows: array<Shadow>;
// The draw-order permutation: the instance array keeps push order, and a draw's instance
// range walks this list.
@group(1) @binding(1) var<storage, read> remap: array<u32>;

struct ShadowVarying {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) instance: u32,
}

@vertex
fn vs_shadow(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32,
) -> ShadowVarying {
    let slot = remap[instance];
    let shadow = shadows[slot];
    // `bounds` is already everything the primitive paints: the blurred shape dilated by the
    // gaussian's reach for a drop shadow, and the casting box itself for an inset one.
    let local = inflated_corner(vertex, shadow.bounds);
    var out: ShadowVarying;
    out.position = to_clip_position(local, shadow.transform);
    out.local = local;
    out.instance = slot;
    return out;
}

// A standard gaussian, used to weight the vertical samples.
fn gaussian(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma)) / (sqrt(2.0 * M_PI) * sigma);
}

// An approximation of the error function, which is the integral the gaussian needs.
fn erf(v: vec2<f32>) -> vec2<f32> {
    let s = sign(v);
    let a = abs(v);
    let r1 = 1.0 + (0.278393 + (0.230389 + (0.000972 + 0.078108 * a) * a) * a) * a;
    let r2 = r1 * r1;
    return s - s / (r2 * r2);
}

// The blurred coverage of one scanline of a rounded rectangle, exactly analytic in x.
//
// `curved` is where the shape's edge sits on this scanline. With an elliptical corner that is the
// ellipse equation rather than the circle's, and it reduces to the circular form when the two
// semi-axes are equal — so the generalisation costs one division and no samples.
fn blur_along_x(x: f32, y: f32, sigma: f32, corner: vec2<f32>, half_size: vec2<f32>) -> f32 {
    let delta = min(half_size.y - corner.y - abs(y), 0.0);
    var curved = half_size.x - corner.x;
    if corner.y > 0.0 {
        let normalised = saturate(1.0 - (delta * delta) / (corner.y * corner.y));
        curved += corner.x * sqrt(normalised);
    } else {
        curved += corner.x;
    }
    let integral = 0.5 + 0.5 * erf((x + vec2<f32>(-curved, curved)) * (sqrt(0.5) / sigma));
    return integral.y - integral.x;
}

@fragment
fn fs_shadow(in: ShadowVarying) -> @location(0) vec4<f32> {
    let shadow = shadows[in.instance];
    let clip = clip_coverage(device_position(in.position.xy), shadow.clip);
    if clip <= 0.0 {
        return vec4<f32>(0.0);
    }

    // The blurred shape is the element box, offset and spread; `bounds` is that shape dilated by
    // the blur's reach, so the shape itself has to be recovered from it.
    let shape = shadow_shape(shadow);
    let half_size = bounds_size(shape) * 0.5;
    let center = bounds_origin(shape) + half_size;
    let center_to_point = in.local - center;
    let corner = pick_corner_radii(center_to_point, shadow.radii);

    var alpha: f32;
    if shadow.blur <= 0.0 {
        alpha = saturate(0.5 - quad_sdf(in.local, shape, shadow.radii));
    } else {
        // The gaussian is negligible beyond three standard deviations, and the shape contributes
        // nothing outside its own extent, so the samples are spent only where the two overlap.
        let low = center_to_point.y - half_size.y;
        let high = center_to_point.y + half_size.y;
        let start = clamp(-3.0 * shadow.blur, low, high);
        let end = clamp(3.0 * shadow.blur, low, high);
        let step = (end - start) / 4.0;
        var y = start + step * 0.5;
        alpha = 0.0;
        for (var i = 0; i < 4; i += 1) {
            let blurred = blur_along_x(
                center_to_point.x,
                center_to_point.y - y,
                shadow.blur,
                corner,
                half_size,
            );
            alpha += blurred * gaussian(y, shadow.blur) * step;
            y += step;
        }
    }

    if shadow.inset != 0u {
        // An inset shadow is the complement of the blurred hole, clipped to the element it sits in.
        alpha = 1.0 - alpha;
        let element_distance = quad_sdf(in.local, shadow.element_bounds, shadow.element_radii);
        alpha *= saturate(0.5 - element_distance);
    } else {
        // An outer shadow is never painted within the box that casts it. Behind a filled box the
        // difference cannot be seen, but a box with no fill of its own — a field that is a hole in
        // the page — would otherwise wear its own shadow as a wash over its whole interior.
        let element_distance = quad_sdf(in.local, shadow.element_bounds, shadow.element_radii);
        alpha *= saturate(0.5 + element_distance);
    }

    return rgba_of(shadow.color) * alpha * clip;
}

// The rectangle the blur is applied to.
//
// A drop shadow's `bounds` is the shape dilated by the blur's reach on every side, so the shape is
// recovered by removing it. An inset shadow paints only inside the box that casts it, so its
// `bounds` is that box and is already the shape the blur is applied to.
fn shadow_shape(shadow: Shadow) -> Bounds {
    if shadow.inset != 0u {
        return shadow.bounds;
    }
    let reach = 3.0 * shadow.blur;
    return Bounds(
        shadow.bounds.x + reach,
        shadow.bounds.y + reach,
        max(shadow.bounds.w - 2.0 * reach, 0.0),
        max(shadow.bounds.h - 2.0 * reach, 0.0),
    );
}
