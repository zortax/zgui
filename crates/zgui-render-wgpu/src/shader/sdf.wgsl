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

// Signed distance from a point mirrored into the bottom-right quadrant to the rounded boundary,
// negative inside. `r` holds the two semi-axes of that quadrant's corner ellipse.
fn quad_sdf_impl(corner_center_to_point: vec2<f32>, r: vec2<f32>) -> f32 {
    let inside_corner = corner_center_to_point.x > 0.0 && corner_center_to_point.y > 0.0;
    if !inside_corner || r.x <= 0.0 || r.y <= 0.0 {
        // Straight edges. With `r.x == r.y` this is the reference's
        // `length(max(0, ccp)) + min(0, max(ccp.x, ccp.y)) - r` on every branch but the corner.
        return max(corner_center_to_point.x - r.x, corner_center_to_point.y - r.y);
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
fn quad_sdf(point: vec2<f32>, bounds: Bounds, radii: Radii) -> f32 {
    let half_size = bounds_size(bounds) * 0.5;
    let center = bounds_origin(bounds) + half_size;
    let center_to_point = point - center;
    let r = pick_corner_radii(center_to_point, radii);
    let corner_to_point = abs(center_to_point) - half_size;
    return quad_sdf_impl(corner_to_point + r, r);
}

// This approximates the distance to the nearest point of a quarter ellipse, which is what the
// inner edge of a border with unequal widths is. Negative outside, positive inside. Modified from
// the reference's average-of-radii scaling to the gradient correction its own comment asks for.
fn quarter_ellipse_sdf(point: vec2<f32>, radii: vec2<f32>) -> f32 {
    if radii.x <= 0.0 || radii.y <= 0.0 {
        return -max(point.x - radii.x, point.y - radii.y);
    }
    let k1 = length(point / radii);
    let k2 = length(point / (radii * radii));
    return -(k1 * (k1 - 1.0) / k2);
}

// Coverage of a device pixel by a rounded rectangle, antialiased over one pixel.
fn rect_coverage(point: vec2<f32>, bounds: Bounds, radii: Radii) -> f32 {
    return saturate(0.5 - quad_sdf(point, bounds, radii));
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
        coverage *= rect_coverage(point, clip.first.rect, clip.first.radii);
    }
    if clip.count > 1u {
        coverage *= rect_coverage(point, clip.second.rect, clip.second.radii);
    }
    return coverage;
}

// Modulus that has the same sign as `a`.
fn sdf_fmod(a: f32, b: f32) -> f32 {
    return a - b * trunc(a / b);
}
