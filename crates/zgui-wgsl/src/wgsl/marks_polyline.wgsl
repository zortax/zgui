// Stroked polylines of a mark payload.
//
// Instance `i` strokes the segment from vertex `i` to vertex `i + 1`, and draws nothing when either
// is a run separator. An end next to a separator is an outer end and takes the item's cap; every
// other end is round, which is the round join.

@group(2) @binding(1) var<storage, read> mark_vertices: array<vec2<f32>>;

fn segment_corner(vertex: u32, instance: u32, coverage: bool) -> MarkVarying {
    let found = item_of(mark_draw.position);
    let item = marks[found.slot];
    let raw_a = mark_vertices[instance];
    let raw_b = mark_vertices[instance + 1u];
    var out: MarkVarying;
    if is_separator(raw_a) || is_separator(raw_b) {
        // A degenerate quad: every corner at one point covers no pixel.
        out.position = vec4<f32>(-2.0, -2.0, 0.0, 1.0);
        out.slot = found.slot;
        out.prim = instance;
        return out;
    }
    let a = to_space(item, raw_a);
    let b = to_space(item, raw_b);
    let along = b - a;
    let length_along = length(along);
    let direction = select(vec2<f32>(1.0, 0.0), along / length_along, length_along > 0.0);
    let normal = vec2<f32>(-direction.y, direction.x);
    let reach = item.half_width + margin_space(item);
    let corner = unit_corner(vertex);
    let lengthwise = select(a - direction * reach, b + direction * reach, corner.x > 0.5);
    let q = lengthwise + normal * select(-reach, reach, corner.y > 0.5);
    out = mark_point(q, item, found.shift, coverage);
    out.slot = found.slot;
    out.prim = instance;
    out.caps = segment_caps(instance, item.flags);
    return out;
}

// The caps of the segment from vertex `at` to vertex `at + 1`: an end beside a separator is an
// outer end and takes the item's cap, and every other end is round.
fn segment_caps(at: u32, flags: u32) -> u32 {
    var start = CAP_ROUND;
    var end = CAP_ROUND;
    if is_separator(mark_vertices[at - 1u]) {
        start = (flags >> MARK_START_CAP_SHIFT) & 3u;
    }
    if is_separator(mark_vertices[at + 2u]) {
        end = (flags >> MARK_END_CAP_SHIFT) & 3u;
    }
    return start | (end << 2u);
}

// Signed distance to the segment from `a` to `b` stroked `half` wide on each side, with `caps`.
fn segment_value(point: vec2<f32>, a: vec2<f32>, b: vec2<f32>, half: f32, caps: u32) -> Distance {
    let along = b - a;
    let length_along = length(along);
    let direction = select(vec2<f32>(1.0, 0.0), along / length_along, length_along > 0.0);
    let normal = vec2<f32>(-direction.y, direction.x);
    let u = dot(point - a, direction);
    let v = dot(point - a, normal);
    let start = caps & 3u;
    let end = (caps >> 2u) & 3u;
    let reach_start = select(0.0, half, start == CAP_SQUARE);
    let reach_end = select(0.0, half, end == CAP_SQUARE);
    // Every side is the near edge of a band: the stroke across, the segment with its caps along.
    let lengthwise = reach_start + length_along + reach_end;
    var before: Distance;
    before.d = -reach_start - u;
    before.gradient = -direction;
    before.band = lengthwise;
    var after: Distance;
    after.d = u - (length_along + reach_end);
    after.gradient = direction;
    after.band = lengthwise;
    var beside: Distance;
    beside.d = abs(v) - half;
    beside.gradient = normal * select(-1.0, 1.0, v >= 0.0);
    beside.band = 2.0 * half;
    var out = distance_max(distance_max(before, after), beside);
    if start == CAP_ROUND {
        var cap = radial(point, a, half);
        cap.band = 2.0 * half;
        out = distance_min(out, cap);
    }
    if end == CAP_ROUND {
        var cap = radial(point, b, half);
        cap.band = 2.0 * half;
        out = distance_min(out, cap);
    }
    return out;
}

// Signed distance to the segment of `in`, and whether that segment is the nearest of its run.
//
// The segments of a run overlap at every join, and a paint draw that painted both would paint
// the overlap twice. So a pixel near a join is drawn only by the nearer of the two segments,
// with that segment's distance, which is the distance to the run there. A coverage draw needs no
// such rule: its bin keeps the largest coverage.
fn segment_distance(in: MarkVarying, owned: ptr<function, bool>) -> Distance {
    let item = marks[in.slot];
    let point = payload_point(in);
    let half = item.half_width;
    let at = in.prim;
    let a = to_space(item, mark_vertices[at]);
    let b = to_space(item, mark_vertices[at + 1u]);
    let own = segment_value(point, a, b, half, in.caps);
    var nearest = true;
    let before = mark_vertices[at - 1u];
    if !is_separator(before) {
        let other = segment_value(
            point,
            to_space(item, before),
            a,
            half,
            segment_caps(at - 1u, item.flags),
        );
        nearest = nearest && own.d < other.d;
    }
    let after = mark_vertices[at + 2u];
    if !is_separator(after) {
        let other = segment_value(
            point,
            b,
            to_space(item, after),
            half,
            segment_caps(at + 1u, item.flags),
        );
        nearest = nearest && own.d <= other.d;
    }
    *owned = nearest;
    return own;
}

@vertex
fn vs_segment_paint(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32,
) -> MarkVarying {
    return segment_corner(vertex, instance, false);
}

@vertex
fn vs_segment_coverage(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32,
) -> MarkVarying {
    return segment_corner(vertex, instance, true);
}

@fragment
fn fs_segment_paint(in: MarkVarying) -> @location(0) vec4<f32> {
    let across = dpdx(in.point);
    let down = dpdy(in.point);
    var owned = true;
    let coverage = sdf_coverage(segment_distance(in, &owned), across, down);
    return mark_paint(in, marks[in.slot], select(0.0, coverage, owned));
}

@fragment
fn fs_segment_coverage(in: MarkVarying) -> @location(0) vec4<f32> {
    let across = dpdx(in.point);
    let down = dpdy(in.point);
    let item = marks[in.slot];
    let a = to_space(item, mark_vertices[in.prim]);
    let b = to_space(item, mark_vertices[in.prim + 1u]);
    // Each quarter is half a pixel wide, and measured from its own centre.
    let point = payload_point(in);
    let half_across = 0.5 * across;
    let half_down = 0.5 * down;
    var quarters = vec4<f32>(0.0);
    for (var quarter = 0u; quarter < 4u; quarter += 1u) {
        let corner = vec2<f32>(f32(quarter & 1u), f32(quarter >> 1u)) - vec2<f32>(0.5);
        let centre = point + corner.x * half_across + corner.y * half_down;
        let distance = segment_value(centre, a, b, item.half_width, in.caps);
        quarters[quarter] = sdf_coverage(distance, half_across, half_down);
    }
    return mark_bin(in, quarters);
}
