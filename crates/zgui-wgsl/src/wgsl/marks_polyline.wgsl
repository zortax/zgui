// Stroked polylines of a mark payload.
//
// Instance `i` strokes the segment from vertex `i` to vertex `i + 1`, and draws nothing when either
// is a run separator. An end next to a separator is an outer end and takes the item's cap; every
// other end is round, which is the round join.

@group(2) @binding(1) var<storage, read> mark_vertices: array<vec2<f32>>;

fn segment_corner(vertex: u32, instance: u32, coverage: bool) -> MarkVarying {
    let found = item_of(mark_draw.position);
    let item = marks[found.slot];
    let a = mark_vertices[instance];
    let b = mark_vertices[instance + 1u];
    var out: MarkVarying;
    if is_separator(a) || is_separator(b) {
        // A degenerate quad: every corner at one point covers no pixel.
        out.position = vec4<f32>(-2.0, -2.0, 0.0, 1.0);
        out.slot = found.slot;
        out.prim = instance;
        return out;
    }
    var start = CAP_ROUND;
    var end = CAP_ROUND;
    if is_separator(mark_vertices[instance - 1u]) {
        start = (item.flags >> MARK_START_CAP_SHIFT) & 3u;
    }
    if is_separator(mark_vertices[instance + 2u]) {
        end = (item.flags >> MARK_END_CAP_SHIFT) & 3u;
    }
    let along = b - a;
    let length_along = length(along);
    let direction = select(vec2<f32>(1.0, 0.0), along / length_along, length_along > 0.0);
    let normal = vec2<f32>(-direction.y, direction.x);
    let reach = item.half_width + margin_local(item.transform);
    let corner = unit_corner(vertex);
    let lengthwise = select(a - direction * reach, b + direction * reach, corner.x > 0.5);
    let payload = lengthwise + normal * select(-reach, reach, corner.y > 0.5);
    let local = payload + vec2<f32>(item.origin.x, item.origin.y) + found.shift;
    if coverage {
        out.position = to_page(local, item.transform);
    } else {
        out.position = to_target(local, item.transform, vec2<f32>(0.0));
    }
    out.local = local;
    out.shift = found.shift;
    out.slot = found.slot;
    out.prim = instance;
    out.caps = start | (end << 2u);
    return out;
}

// Signed distance to one stroked segment with its caps, negative inside.
fn segment_distance(in: MarkVarying) -> f32 {
    let item = marks[in.slot];
    let a = mark_vertices[in.prim];
    let b = mark_vertices[in.prim + 1u];
    let point = payload_point(in, item);
    let half = item.half_width;
    let along = b - a;
    let length_along = length(along);
    let direction = select(vec2<f32>(1.0, 0.0), along / length_along, length_along > 0.0);
    let normal = vec2<f32>(-direction.y, direction.x);
    let u = dot(point - a, direction);
    let v = dot(point - a, normal);
    let start = in.caps & 3u;
    let end = (in.caps >> 2u) & 3u;
    let reach_start = select(0.0, half, start == CAP_SQUARE);
    let reach_end = select(0.0, half, end == CAP_SQUARE);
    var d = max(max(-reach_start - u, u - (length_along + reach_end)), abs(v) - half);
    if start == CAP_ROUND {
        d = min(d, length(point - a) - half);
    }
    if end == CAP_ROUND {
        d = min(d, length(point - b) - half);
    }
    return d;
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
    let coverage = sdf_coverage(segment_distance(in));
    return mark_paint(in, marks[in.slot], coverage);
}

@fragment
fn fs_segment_coverage(in: MarkVarying) -> @location(0) vec4<f32> {
    let coverage = sdf_coverage(segment_distance(in));
    return mark_bin(in, coverage);
}
