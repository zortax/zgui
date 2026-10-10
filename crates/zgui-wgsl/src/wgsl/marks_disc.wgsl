// Discs and rings of a mark payload.

@group(2) @binding(1) var<storage, read> mark_discs: array<vec4<f32>>;

fn disc_corner(vertex: u32, instance: u32, coverage: bool) -> MarkVarying {
    let found = item_of(mark_draw.position);
    let item = marks[found.slot];
    let disc = mark_discs[instance];
    let centre = to_space(item, disc.xy);
    // The quad holds a square of half side `outer` as well as a disc.
    let reach = disc.z + margin_space(item);
    let bounds = vec4<f32>(centre - vec2<f32>(reach), centre + vec2<f32>(reach));
    var out = mark_corner(vertex, bounds, item, found.shift, coverage);
    out.slot = found.slot;
    out.prim = instance;
    return out;
}

// Signed distance to a disc of radius z with a hole of radius w, negative inside.
//
// A ring is a band z − w wide, so a pixel that holds both edges of a thin ring covers its width.
fn disc_distance(in: MarkVarying) -> Distance {
    let item = marks[in.slot];
    let disc = mark_discs[in.prim];
    var outer = radial(payload_point(in), to_space(item, disc.xy), disc.z);
    if disc.w <= 0.0 {
        return outer;
    }
    outer.band = disc.z - disc.w;
    var hole: Distance;
    hole.d = disc.w - (outer.d + disc.z);
    hole.gradient = -outer.gradient;
    hole.band = outer.band;
    return distance_max(outer, hole);
}

// The coverage of an axis-aligned square of half side `half` around `centre`: the product of
// the coverage across each axis, as along a side of a box.
fn square_cover(point: vec2<f32>, centre: vec2<f32>, half: f32, across: vec2<f32>, down: vec2<f32>) -> f32 {
    let from_centre = point - centre;
    let flip = select(vec2<f32>(-1.0), vec2<f32>(1.0), from_centre >= vec2<f32>(0.0));
    let corner = abs(from_centre) - vec2<f32>(half);
    var x: Distance;
    x.d = corner.x;
    x.gradient = vec2<f32>(flip.x, 0.0);
    x.band = 0.0;
    var y: Distance;
    y.d = corner.y;
    y.gradient = vec2<f32>(0.0, flip.y);
    y.band = 0.0;
    return sdf_coverage(x, across, down) * sdf_coverage(y, across, down);
}

// The coverage of the disc or square of `in`.
fn disc_coverage(in: MarkVarying, across: vec2<f32>, down: vec2<f32>) -> f32 {
    let item = marks[in.slot];
    if (item.flags & MARK_SQUARE_DISCS) == 0u {
        return sdf_coverage(disc_distance(in), across, down);
    }
    let disc = mark_discs[in.prim];
    let centre = to_space(item, disc.xy);
    let point = payload_point(in);
    let outer = square_cover(point, centre, disc.z, across, down);
    if disc.w <= 0.0 {
        return outer;
    }
    return saturate(outer - square_cover(point, centre, disc.w, across, down));
}

@vertex
fn vs_disc_paint(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32,
) -> MarkVarying {
    return disc_corner(vertex, instance, false);
}

@vertex
fn vs_disc_coverage(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32,
) -> MarkVarying {
    return disc_corner(vertex, instance, true);
}

@fragment
fn fs_disc_paint(in: MarkVarying) -> @location(0) vec4<f32> {
    let across = dpdx(in.point);
    let down = dpdy(in.point);
    let coverage = disc_coverage(in, across, down);
    return mark_paint(in, marks[in.slot], coverage);
}

@fragment
fn fs_disc_coverage(in: MarkVarying) -> @location(0) vec4<f32> {
    let across = dpdx(in.point);
    let down = dpdy(in.point);
    let coverage = disc_coverage(in, across, down);
    return mark_bin(in, vec4<f32>(coverage));
}
