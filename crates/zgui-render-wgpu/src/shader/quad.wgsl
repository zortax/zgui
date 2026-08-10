// DERIVED-FROM: the GPUI project, crates/gpui_wgpu/src/shaders.wgsl (Apache-2.0)
// The quad fragment shader — the inner-edge signed distance, the border blend, and the dash
// parameterisation that lays dashes out clockwise around the whole perimeter — is adapted from
// that work, which is licensed under the Apache License, Version 2.0, and has been modified: every
// corner radius is a pair of elliptical semi-axes rather than a scalar, so each straight side takes
// the radii of its own axis, each quarter corner's arc length is a Ramanujan quarter-ellipse
// perimeter rather than `r * pi / 2`, and the position along a corner uses the eccentric anomaly.
// The background is a paint-table reference rather than an inline two-stop gradient, the clip is a
// chain evaluated by a shared coverage function rather than four interpolated distances, and dotted
// borders are a second style rather than an unimplemented one.

struct Quad {
    order: u32,
    style: u32,
    bounds: Bounds,
    radii: Radii,
    border: Edges,
    fill: PaintRef,
    stroke: PaintRef,
    clip: u32,
    transform: u32,
    // Where the space the two paints were resolved in has its origin, subtracted from the sample
    // point before either is evaluated. Zero for a quad drawn where its paints were resolved.
    paint_origin: Vector2,
}

@group(1) @binding(0) var<storage, read> quads: array<Quad>;
// The draw-order permutation: the instance array keeps push order, and a draw's instance
// range walks this list.
@group(1) @binding(1) var<storage, read> remap: array<u32>;

const BORDER_SOLID: u32 = 0u;
const BORDER_DASHED: u32 = 1u;
const BORDER_DOTTED: u32 = 2u;

struct QuadVarying {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) instance: u32,
}

@vertex
fn vs_quad(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32,
) -> QuadVarying {
    let slot = remap[instance];
    let quad = quads[slot];
    let local = inflated_corner(vertex, quad.bounds);
    var out: QuadVarying;
    out.position = to_clip_position(local, quad.transform);
    out.local = local;
    out.instance = slot;
    return out;
}

@fragment
fn fs_quad(in: QuadVarying) -> @location(0) vec4<f32> {
    let quad = quads[in.instance];
    // The clip is in device space, so it is evaluated at the real pixel; the shape is in the
    // primitive's own space, so it is evaluated at the point that maps to this pixel.
    let clip = clip_coverage(device_position(in.position.xy), quad.clip);
    if clip <= 0.0 {
        return vec4<f32>(0.0);
    }
    let point = in.local;
    let paint_origin = vec2<f32>(quad.paint_origin.x, quad.paint_origin.y);
    let background = paint_color(quad.fill, point, paint_origin);

    let size = bounds_size(quad.bounds);
    let half_size = size * 0.5;
    let center_to_point = point - (bounds_origin(quad.bounds) + half_size);

    // Half a pixel is the largest distance between a pixel's centre and an edge that covers it.
    let antialias_threshold = 0.5;

    let corner_to_point = abs(center_to_point) - half_size;
    let corner_radii = pick_corner_radii(center_to_point, quad.radii);
    let unrounded = quad.radii.tl_x == 0.0 && quad.radii.tl_y == 0.0
        && quad.radii.tr_x == 0.0 && quad.radii.tr_y == 0.0
        && quad.radii.br_x == 0.0 && quad.radii.br_y == 0.0
        && quad.radii.bl_x == 0.0 && quad.radii.bl_y == 0.0;
    let no_border = quad.border.top == 0.0 && quad.border.right == 0.0
        && quad.border.bottom == 0.0 && quad.border.left == 0.0;

    if unrounded && no_border {
        // Still antialiased, and it matters more than it looks: every corner of the quad is
        // expanded by a pixel so that a partly covered edge has somewhere to land, so returning
        // the background unweighted here would paint a full-intensity ring one pixel outside every
        // plain rectangle in the frame.
        let square_sdf = max(corner_to_point.x, corner_to_point.y);
        return background * saturate(antialias_threshold - square_sdf) * clip;
    }

    // The widths of the two nearest sides.
    let border = vec2<f32>(
        select(quad.border.right, quad.border.left, center_to_point.x < 0.0),
        select(quad.border.bottom, quad.border.top, center_to_point.y < 0.0),
    );
    // A zero-width side is pushed outside the antialiasing band so that no partial pixel is drawn
    // for a border that is not there.
    let reduced_border = vec2<f32>(
        select(border.x, -antialias_threshold, border.x == 0.0),
        select(border.y, -antialias_threshold, border.y == 0.0),
    );

    let corner_center_to_point = corner_to_point + corner_radii;
    let is_near_rounded_corner = corner_center_to_point.x >= 0.0 && corner_center_to_point.y >= 0.0;
    let straight_inner_corner_to_point = corner_to_point + reduced_border;
    let is_beyond_inner_straight_border = straight_inner_corner_to_point.x > 0.0
        || straight_inner_corner_to_point.y > 0.0;
    let is_within_inner_straight_border = straight_inner_corner_to_point.x < -antialias_threshold
        && straight_inner_corner_to_point.y < -antialias_threshold;

    if is_within_inner_straight_border && !is_near_rounded_corner {
        return background * clip;
    }

    // Positive outside the outer edge of the border, negative inside it.
    let outer_sdf = quad_sdf_impl(corner_center_to_point, corner_radii);

    // Positive inside the inner edge of the border, negative within the border itself.
    var inner_sdf = 0.0;
    if corner_center_to_point.x <= 0.0 || corner_center_to_point.y <= 0.0 {
        inner_sdf = -max(straight_inner_corner_to_point.x, straight_inner_corner_to_point.y);
    } else if is_beyond_inner_straight_border {
        inner_sdf = -1.0;
    } else if reduced_border.x == reduced_border.y && corner_radii.x == corner_radii.y {
        // Circular inner edge: the outer distance shifted inwards is exact.
        inner_sdf = -(outer_sdf + reduced_border.x);
    } else {
        let ellipse_radii = max(vec2<f32>(0.0), corner_radii - reduced_border);
        inner_sdf = quarter_ellipse_sdf(corner_center_to_point, ellipse_radii);
    }

    let border_sdf = max(inner_sdf, outer_sdf);

    var color = background;
    if border_sdf < antialias_threshold {
        var border_color = paint_color(quad.stroke, point, paint_origin);
        let style = quad.style & 0xffu;
        if style != BORDER_SOLID {
            border_color *= dash_coverage(
                quad,
                style,
                point,
                center_to_point,
                corner_center_to_point,
                corner_radii,
                is_near_rounded_corner,
                unrounded,
                antialias_threshold,
            );
        }
        let blended = over_premultiplied(background, border_color);
        color = mix(background, blended, saturate(antialias_threshold - inner_sdf));
    }

    return color * saturate(antialias_threshold - outer_sdf) * clip;
}

// Premultiplied source-over.
fn over_premultiplied(below: vec4<f32>, above: vec4<f32>) -> vec4<f32> {
    return above + below * (1.0 - above.a);
}

// Ramanujan's approximation of a quarter-ellipse's arc length, which reduces to `r * pi / 2` when
// the two semi-axes are equal.
fn quarter_ellipse_perimeter(r: vec2<f32>) -> f32 {
    if r.x <= 0.0 && r.y <= 0.0 {
        return 0.0;
    }
    let h = ((r.x - r.y) * (r.x - r.y)) / max((r.x + r.y) * (r.x + r.y), 1e-6);
    return 0.25 * M_PI * (r.x + r.y) * (1.0 + (3.0 * h) / (10.0 + sqrt(max(4.0 - 3.0 * h, 0.0))));
}

// The slower of two dash velocities, so a corner takes the larger dashes of the two sides it
// joins. A zero velocity means a zero-width side, which contributes nothing.
fn corner_dash_velocity(first: f32, second: f32) -> f32 {
    if first == 0.0 {
        return second;
    }
    if second == 0.0 {
        return first;
    }
    return min(first, second);
}

// Coverage of a dash at position `t` in dash space, where one dash period has length one.
fn dash_alpha(t: f32, period: f32, length: f32, velocity: f32, threshold: f32) -> f32 {
    let half_period = period * 0.5;
    let half_length = length * 0.5;
    let centered = sdf_fmod(t + half_period - half_length, period) - half_period;
    let signed_distance = abs(centered) - half_length;
    return saturate(threshold - signed_distance / max(velocity, 1e-6));
}

// How much of this pixel a dashed or dotted border covers.
//
// Dash size is proportional to border width, which is what browsers do and what keeps dashes from
// overlapping where the border is thicker than the dash. A dotted border is the same machinery
// with a one-to-one dash and gap.
fn dash_coverage(
    quad: Quad,
    style: u32,
    point: vec2<f32>,
    center_to_point: vec2<f32>,
    corner_center_to_point: vec2<f32>,
    corner_radii: vec2<f32>,
    is_near_rounded_corner: bool,
    unrounded: bool,
    threshold: f32,
) -> f32 {
    let dash_per_width = select(2.0, 1.0, style == BORDER_DOTTED);
    let gap_per_width = 1.0;
    let period_per_width = dash_per_width + gap_per_width;
    let dv_numerator = 1.0 / period_per_width;

    let size = bounds_size(quad.bounds);
    let origin = bounds_origin(quad.bounds);
    let local = point - origin;

    var t = 0.0;
    var max_t = 0.0;
    var velocity = 0.0;

    if unrounded {
        // Without rounded corners each side lays its dashes out on its own, so every side starts
        // and ends with a dash.
        let is_horizontal = corner_center_to_point.x < corner_center_to_point.y;
        let widths = vec2<f32>(
            max(quad.border.bottom, quad.border.top),
            max(quad.border.right, quad.border.left),
        );
        let width = select(widths.y, widths.x, is_horizontal);
        velocity = select(0.0, dv_numerator / width, width > 0.0);
        t = select(local.y, local.x, is_horizontal) * velocity;
        max_t = select(size.y, size.x, is_horizontal) * velocity;
    } else {
        let r_tl = vec2<f32>(quad.radii.tl_x, quad.radii.tl_y);
        let r_tr = vec2<f32>(quad.radii.tr_x, quad.radii.tr_y);
        let r_br = vec2<f32>(quad.radii.br_x, quad.radii.br_y);
        let r_bl = vec2<f32>(quad.radii.bl_x, quad.radii.bl_y);

        let dv_t = select(0.0, dv_numerator / quad.border.top, quad.border.top > 0.0);
        let dv_r = select(0.0, dv_numerator / quad.border.right, quad.border.right > 0.0);
        let dv_b = select(0.0, dv_numerator / quad.border.bottom, quad.border.bottom > 0.0);
        let dv_l = select(0.0, dv_numerator / quad.border.left, quad.border.left > 0.0);

        // A straight side runs between the two corners on its own axis, so it takes the x radii of
        // the horizontal sides and the y radii of the vertical ones.
        let s_t = max(size.x - r_tl.x - r_tr.x, 0.0) * dv_t;
        let s_r = max(size.y - r_tr.y - r_br.y, 0.0) * dv_r;
        let s_b = max(size.x - r_br.x - r_bl.x, 0.0) * dv_b;
        let s_l = max(size.y - r_bl.y - r_tl.y, 0.0) * dv_l;

        let cv_tr = corner_dash_velocity(dv_t, dv_r);
        let cv_br = corner_dash_velocity(dv_b, dv_r);
        let cv_bl = corner_dash_velocity(dv_b, dv_l);
        let cv_tl = corner_dash_velocity(dv_t, dv_l);

        let c_tr = quarter_ellipse_perimeter(r_tr) * cv_tr;
        let c_br = quarter_ellipse_perimeter(r_br) * cv_br;
        let c_bl = quarter_ellipse_perimeter(r_bl) * cv_bl;
        let c_tl = quarter_ellipse_perimeter(r_tl) * cv_tl;

        let upto_tr = s_t;
        let upto_r = upto_tr + c_tr;
        let upto_br = upto_r + s_r;
        let upto_b = upto_br + c_br;
        let upto_bl = upto_b + s_b;
        let upto_l = upto_bl + c_bl;
        let upto_tl = upto_l + s_l;
        max_t = upto_tl + c_tl;

        if is_near_rounded_corner {
            // The eccentric anomaly, scaled by the corner's own arc length, is what keeps the dash
            // rhythm continuous across an elliptical corner: the angle alone would run fast on the
            // short axis and slow on the long one.
            let radii = max(corner_radii, vec2<f32>(1e-6));
            let anomaly = atan2(corner_center_to_point.y / radii.y, corner_center_to_point.x / radii.x);
            let fraction = saturate(anomaly / (0.5 * M_PI));

            if center_to_point.x >= 0.0 {
                if center_to_point.y < 0.0 {
                    velocity = cv_tr;
                    t = upto_r - fraction * c_tr;
                } else {
                    velocity = cv_br;
                    t = upto_br + fraction * c_br;
                }
            } else {
                if center_to_point.y >= 0.0 {
                    velocity = cv_bl;
                    t = upto_l - fraction * c_bl;
                } else {
                    velocity = cv_tl;
                    t = upto_tl + fraction * c_tl;
                }
            }
        } else {
            let is_horizontal = corner_center_to_point.x < corner_center_to_point.y;
            if is_horizontal {
                if center_to_point.y < 0.0 {
                    velocity = dv_t;
                    t = (local.x - r_tl.x) * velocity;
                } else {
                    velocity = dv_b;
                    t = upto_bl - (local.x - r_bl.x) * velocity;
                }
            } else {
                if center_to_point.x < 0.0 {
                    velocity = dv_l;
                    t = upto_tl - (local.y - r_tl.y) * velocity;
                } else {
                    velocity = dv_r;
                    t = upto_r + (local.y - r_tr.y) * velocity;
                }
            }
        }
    }

    let dash_length = dash_per_width / period_per_width;
    // A straight run starts and ends with a dash, which is what shortening its extent by one dash
    // before dividing achieves.
    max_t -= select(0.0, dash_length, unrounded);
    if max_t >= 1.0 {
        let dash_count = floor(max_t);
        return dash_alpha(t, max_t / dash_count, dash_length, velocity, threshold);
    }
    if unrounded {
        let dash_gap = max_t - dash_length;
        if dash_gap > 0.0 {
            return dash_alpha(t, dash_length + dash_gap, dash_length, velocity, threshold);
        }
    }
    return 1.0;
}
