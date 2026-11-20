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
    shape_bounds: Bounds,
    radii: Radii,
    element_bounds: Bounds,
    element_radii: Radii,
    color: Rgba,
    clip: u32,
    transform: u32,
    inset: u32,
    // The superellipse exponent the element's corners are cut with; two is the ellipse.
    shape: f32,
}

@group(1) @binding(0) var shadows: texture_2d<u32>;

/// One `Shadow`, which the arena pads to 10 texels.
fn load_shadow(slot: u32) -> Shadow {
    let base = slot * 10u;
    let t0 = textureLoad(shadows, table_texel(base + 0u), 0);
    let t1 = textureLoad(shadows, table_texel(base + 1u), 0);
    let t2 = textureLoad(shadows, table_texel(base + 2u), 0);
    let t3 = textureLoad(shadows, table_texel(base + 3u), 0);
    let t4 = textureLoad(shadows, table_texel(base + 4u), 0);
    let t5 = textureLoad(shadows, table_texel(base + 5u), 0);
    let t6 = textureLoad(shadows, table_texel(base + 6u), 0);
    let t7 = textureLoad(shadows, table_texel(base + 7u), 0);
    let t8 = textureLoad(shadows, table_texel(base + 8u), 0);
    let t9 = textureLoad(shadows, table_texel(base + 9u), 0);
    return Shadow(
        t0.x,
        bitcast<f32>(t0.y),
        Bounds(bitcast<f32>(t0.z), bitcast<f32>(t0.w), bitcast<f32>(t1.x), bitcast<f32>(t1.y)),
        Bounds(bitcast<f32>(t1.z), bitcast<f32>(t1.w), bitcast<f32>(t2.x), bitcast<f32>(t2.y)),
        Radii(bitcast<f32>(t2.z), bitcast<f32>(t2.w), bitcast<f32>(t3.x), bitcast<f32>(t3.y), bitcast<f32>(t3.z), bitcast<f32>(t3.w), bitcast<f32>(t4.x), bitcast<f32>(t4.y)),
        Bounds(bitcast<f32>(t4.z), bitcast<f32>(t4.w), bitcast<f32>(t5.x), bitcast<f32>(t5.y)),
        Radii(bitcast<f32>(t5.z), bitcast<f32>(t5.w), bitcast<f32>(t6.x), bitcast<f32>(t6.y), bitcast<f32>(t6.z), bitcast<f32>(t6.w), bitcast<f32>(t7.x), bitcast<f32>(t7.y)),
        Rgba(bitcast<f32>(t7.z), bitcast<f32>(t7.w), bitcast<f32>(t8.x), bitcast<f32>(t8.y)),
        t8.z,
        t8.w,
        t9.x,
        bitcast<f32>(t9.y),
    );
}

struct ShadowVarying {
    @builtin(position) position: vec4<f32>,
    // The point, from the shadow's origin in the space it was encoded in.
    @location(0) local: vec2<f32>,
    // The shadow's origin in that space.
    @location(1) @interpolate(flat) origin: vec2<f32>,
    // The record the fragment stage used to read again, carried across instead.
    @location(2) @interpolate(flat) v_shape_bounds: vec4<f32>,
    @location(3) @interpolate(flat) v_radii_lo: vec4<f32>,
    @location(4) @interpolate(flat) v_radii_hi: vec4<f32>,
    @location(5) @interpolate(flat) v_element_bounds: vec4<f32>,
    @location(6) @interpolate(flat) v_element_radii_lo: vec4<f32>,
    @location(7) @interpolate(flat) v_element_radii_hi: vec4<f32>,
    @location(8) @interpolate(flat) v_color: vec4<f32>,
    @location(9) @interpolate(flat) v_blur_shape: vec2<f32>,
    @location(10) @interpolate(flat) v_clip_inset: vec2<u32>,
}

@vertex
fn vs_shadow(
    @builtin(vertex_index) vertex: u32,
    @location(0) slot: u32,
    @location(1) shift: vec2<f32>,
) -> ShadowVarying {
    let shadow = load_shadow(slot);
    // `bounds` is already everything the primitive paints: the blurred shape dilated by the
    // gaussian's reach for a drop shadow, and the casting box itself for an inset one.
    let local = inflated_corner(vertex, shadow.bounds) + shift;
    var out: ShadowVarying;
    out.position = to_clip_position(local, shadow.transform);
    out.local = inflated_offset(vertex, shadow.bounds);
    out.origin = bounds_origin(shadow.bounds);
    out.v_shape_bounds = bounds_of(shadow.shape_bounds);
    out.v_radii_lo = radii_lo(shadow.radii);
    out.v_radii_hi = radii_hi(shadow.radii);
    out.v_element_bounds = bounds_of(shadow.element_bounds);
    out.v_element_radii_lo = radii_lo(shadow.element_radii);
    out.v_element_radii_hi = radii_hi(shadow.element_radii);
    out.v_color = rgba_of(shadow.color);
    out.v_blur_shape = vec2<f32>(shadow.blur, shadow.shape);
    out.v_clip_inset = vec2<u32>(shadow.clip, shadow.inset);
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
// semi-axes are equal — so the generalisation costs one division and no samples. A corner cut to
// any other exponent takes the superellipse's own edge, which reduces to the ellipse's at two.
fn blur_along_x(
    x: f32,
    y: f32,
    sigma: f32,
    corner: vec2<f32>,
    half_size: vec2<f32>,
    shape: f32,
) -> f32 {
    let delta = min(half_size.y - corner.y - abs(y), 0.0);
    var curved = half_size.x - corner.x;
    if corner.y > 0.0 {
        if shape == CORNER_ROUND {
            let normalised = saturate(1.0 - (delta * delta) / (corner.y * corner.y));
            curved += corner.x * sqrt(normalised);
        } else {
            // `|x/rx|^n + |y/ry|^n = 1` solved for x, which is the ellipse's own solution at two.
            let power = clamp(shape, 0.01, 64.0);
            let along = saturate(abs(delta) / corner.y);
            let remaining = saturate(1.0 - pow(along, power));
            curved += corner.x * pow(remaining, 1.0 / power);
        }
    } else {
        curved += corner.x;
    }
    let integral = 0.5 + 0.5 * erf((x + vec2<f32>(-curved, curved)) * (sqrt(0.5) / sigma));
    return integral.y - integral.x;
}

@fragment
fn fs_shadow(in: ShadowVarying) -> @location(0) vec4<f32> {
    let clip = clip_coverage(device_position(in.position.xy), in.v_clip_inset.x);
    if clip <= 0.0 {
        return vec4<f32>(0.0);
    }

    // The blurred shape: the element box offset and grown by the spread for a drop shadow, and the
    // hole the shadow leaves open for an inset one. All distances are from the shadow's origin, so
    // they keep their precision far from the origin of the space.
    let casting = bounds_from(bounds_from_vec4(in.v_shape_bounds), in.origin);
    let element = bounds_from(bounds_from_vec4(in.v_element_bounds), in.origin);
    let half_size = bounds_size(casting) * 0.5;
    let center = bounds_origin(casting) + half_size;
    let local = in.local;
    let center_to_point = local - center;
    let radii = radii_from_vec4(in.v_radii_lo, in.v_radii_hi);
    let element_radii = radii_from_vec4(in.v_element_radii_lo, in.v_element_radii_hi);
    let corner = pick_corner_radii(center_to_point, radii);

    // An outer shadow paints nothing inside the box that casts it — the multiply at the end of
    // this function says so, having already paid for the integral. Settled here instead, before
    // four samples of two error functions and an exponential apiece.
    let element_distance = quad_sdf(local, element, element_radii, in.v_blur_shape.y);
    if in.v_clip_inset.y == 0u && element_distance <= -0.5 {
        return vec4<f32>(0.0);
    }

    var alpha: f32;
    if in.v_blur_shape.x <= 0.0 {
        alpha = saturate(0.5 - quad_sdf(local, casting, radii, in.v_blur_shape.y));
    } else {
        // The gaussian is negligible beyond three standard deviations, and the shape contributes
        // nothing outside its own extent, so the samples are spent only where the two overlap.
        let low = center_to_point.y - half_size.y;
        let high = center_to_point.y + half_size.y;
        let start = clamp(-3.0 * in.v_blur_shape.x, low, high);
        let end = clamp(3.0 * in.v_blur_shape.x, low, high);
        let step = (end - start) / 4.0;
        var y = start + step * 0.5;
        alpha = 0.0;
        for (var i = 0; i < 4; i += 1) {
            let blurred = blur_along_x(
                center_to_point.x,
                center_to_point.y - y,
                in.v_blur_shape.x,
                corner,
                half_size,
                in.v_blur_shape.y,
            );
            alpha += blurred * gaussian(y, in.v_blur_shape.x) * step;
            y += step;
        }
    }

    if in.v_clip_inset.y != 0u {
        // An inset shadow is the complement of the blurred hole, clipped to the element it sits in.
        alpha = 1.0 - alpha;
        alpha *= saturate(0.5 - element_distance);
    } else {
        // An outer shadow is never painted within the box that casts it. Behind a filled box the
        // difference cannot be seen, but a box with no fill of its own — a field that is a hole in
        // the page — would otherwise wear its own shadow as a wash over its whole interior.
        alpha *= saturate(0.5 + element_distance);
    }

    return in.v_color * alpha * clip;
}

