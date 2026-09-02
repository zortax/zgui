// DERIVED-FROM: the GPUI project, crates/gpui_wgpu/src/shaders.wgsl (Apache-2.0)
// The rounded-rectangle signed distance function, the quadrant-based corner selection and the
// half-pixel coverage rule are adapted from that work, which is licensed under the
// Apache License, Version 2.0, and have been modified: the scalar corner radius is generalised to
// a pair of elliptical semi-axes per corner, so `border-radius: 80px / 20px` is expressible, and
// the circular case is kept as the exact fast path.

// Selects the corner radii of the quadrant the point lies in.
fn pick_corner_radii(center_to_point: vec2<f32>, radii: Radii) -> vec2<f32> {
    if center_to_point.x < 0.0 {
        if center_to_point.y < 0.0 {
            return vec2<f32>(radii.tl_x, radii.tl_y);
        }
        return vec2<f32>(radii.bl_x, radii.bl_y);
    }
    if center_to_point.y < 0.0 {
        return vec2<f32>(radii.tr_x, radii.tr_y);
    }
    return vec2<f32>(radii.br_x, radii.br_y);
}

// The exponent an ordinary corner radius has always been cut with.
//
// A corner is a superellipse quadrant `|x/rx|^n + |y/ry|^n = 1`, and two is the ellipse. Every
// other shape a box can ask for is another exponent — one is a straight bevel, four a squircle,
// below one a scoop, large a square — so the shading branches on a number rather than on a case
// per name.
const CORNER_ROUND: f32 = 2.0;

// Signed distance from a point mirrored into the bottom-right quadrant to a superellipse corner,
// negative inside, for any exponent but the ellipse's.
//
// `f(p) = (|x/rx|^n + |y/ry|^n)^(1/n)` is one on the boundary; dividing `f - 1` by the magnitude
// of its own gradient turns it into a first-order distance, which is exact on both axes and close
// everywhere between. At `n = 2` it reduces algebraically to the elliptical form above it — which
// is why that one is kept as its own branch rather than deleted: `pow(x, 2.0)` and `x * x` are not
// the same float, and every rounded box already drawn has to keep the pixels it has.
fn superellipse_corner_sdf(corner_center_to_point: vec2<f32>, r: vec2<f32>, n: f32) -> f32 {
    // The exponent reaches here from an instance field, so it is held inside what `pow` can
    // evaluate rather than trusted: a zero would make every gradient infinite.
    let power = clamp(n, 0.01, 64.0);
    // Away from zero on both axes, because the gradient raises this to `power - 1`, which is
    // negative for a scooped corner and infinite at zero.
    let normalised = max(abs(corner_center_to_point) / r, vec2<f32>(1.0e-6));
    let raised = pow(normalised, vec2<f32>(power));
    let sum = max(raised.x + raised.y, 1.0e-12);
    let value = pow(sum, 1.0 / power);
    // |grad f|, with the shared `sum^(1/n - 1)` factored out of both components.
    let slope = pow(normalised, vec2<f32>(power - 1.0)) / r;
    let gradient = pow(sum, 1.0 / power - 1.0) * length(slope);
    return (value - 1.0) / max(gradient, 1.0e-6);
}

// Signed distance from a point mirrored into the bottom-right quadrant to the rounded boundary,
// negative inside. `r` holds the two semi-axes of that quadrant's corner, and `shape` the exponent
// it is cut with.
fn quad_sdf_impl(corner_center_to_point: vec2<f32>, r: vec2<f32>, shape: f32) -> f32 {
    let inside_corner = corner_center_to_point.x > 0.0 && corner_center_to_point.y > 0.0;
    if !inside_corner || r.x <= 0.0 || r.y <= 0.0 {
        // Straight edges. With `r.x == r.y` this is the reference's
        // `length(max(0, ccp)) + min(0, max(ccp.x, ccp.y)) - r` on every branch but the corner.
        // The shape says nothing here: there is no corner to cut.
        return max(corner_center_to_point.x - r.x, corner_center_to_point.y - r.y);
    }
    if shape != CORNER_ROUND {
        return superellipse_corner_sdf(corner_center_to_point, r, shape);
    }
    if r.x == r.y {
        // Exact circular form, kept as the fast path.
        return length(corner_center_to_point) - r.x;
    }
    // Elliptical corner. `f(p) = length(p / r) - 1` is zero on the ellipse; dividing by the
    // gradient magnitude `k2 / k1` turns it into a first-order distance, exact on both axes.
    let k1 = length(corner_center_to_point / r);
    let k2 = length(corner_center_to_point / (r * r));
    return k1 * (k1 - 1.0) / k2;
}

// Signed distance from a device-space point to a rounded rectangle, positive outside.
fn quad_sdf(point: vec2<f32>, bounds: Bounds, radii: Radii, shape: f32) -> f32 {
    let half_size = bounds_size(bounds) * 0.5;
    let center = bounds_origin(bounds) + half_size;
    let center_to_point = point - center;
    let r = pick_corner_radii(center_to_point, radii);
    let corner_to_point = abs(center_to_point) - half_size;
    return quad_sdf_impl(corner_to_point + r, r, shape);
}

// This approximates the distance to the nearest point of a quarter corner, which is what the inner
// edge of a border with unequal widths is. Negative outside, positive inside. Modified from the
// reference's average-of-radii scaling to the gradient correction its own comment asks for, and
// generalised to the shape the outer edge is cut with — a bevelled box has a bevelled border, and
// a border that stayed elliptical inside a squircle would be a ring of uneven width.
fn quarter_ellipse_sdf(point: vec2<f32>, radii: vec2<f32>, shape: f32) -> f32 {
    if radii.x <= 0.0 || radii.y <= 0.0 {
        return -max(point.x - radii.x, point.y - radii.y);
    }
    if shape != CORNER_ROUND {
        return -superellipse_corner_sdf(point, radii, shape);
    }
    let k1 = length(point / radii);
    let k2 = length(point / (radii * radii));
    return -(k1 * (k1 - 1.0) / k2);
}

// Coverage of a device pixel by a rounded rectangle, antialiased over one pixel.
fn rect_coverage(point: vec2<f32>, bounds: Bounds, radii: Radii, shape: f32) -> f32 {
    return saturate(0.5 - quad_sdf(point, bounds, radii, shape));
}

// Coverage of a device pixel by a whole clip chain. Every pipeline that draws into the composed
// target applies exactly this function, so one clip means one thing whatever draws through it.
fn clip_coverage(point: vec2<f32>, clip_id: u32) -> f32 {
    let clip = clips[clip_id];
    // The intersection rectangle is a hard edge: it is an axis-aligned box in device space, and
    // antialiasing it would bleed content one pixel outside a scrollport.
    let aabb = clip.aabb;
    if point.x < aabb.x || point.y < aabb.y
        || point.x > aabb.x + aabb.w || point.y > aabb.y + aabb.h {
        return 0.0;
    }
    var coverage = 1.0;
    if clip.count > 0u {
        coverage *= rect_coverage(point, clip.first.rect, clip.first.radii, clip.first.shape);
    }
    if clip.count > 1u {
        coverage *= rect_coverage(point, clip.second.rect, clip.second.radii, clip.second.shape);
    }
    return coverage;
}

// Modulus that has the same sign as `a`.
fn sdf_fmod(a: f32, b: f32) -> f32 {
    return a - b * trunc(a / b);
}
