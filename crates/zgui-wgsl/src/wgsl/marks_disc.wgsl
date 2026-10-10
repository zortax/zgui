// Discs and rings of a mark payload.

@group(2) @binding(1) var<storage, read> mark_discs: array<vec4<f32>>;

fn disc_corner(vertex: u32, instance: u32, coverage: bool) -> MarkVarying {
    let found = item_of(mark_draw.position);
    let item = marks[found.slot];
    let disc = mark_discs[instance];
    let reach = disc.z + margin_local(item.transform);
    let bounds = vec4<f32>(disc.xy - vec2<f32>(reach), disc.xy + vec2<f32>(reach));
    var out = mark_corner(vertex, bounds, item, found.shift, coverage);
    out.slot = found.slot;
    out.prim = instance;
    return out;
}

// Signed distance to a disc of radius z with a hole of radius w, negative inside.
fn disc_distance(in: MarkVarying) -> f32 {
    let item = marks[in.slot];
    let disc = mark_discs[in.prim];
    let radius = length(payload_point(in, item) - disc.xy);
    let outer = radius - disc.z;
    return select(outer, max(outer, disc.w - radius), disc.w > 0.0);
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
    let coverage = sdf_coverage(disc_distance(in));
    return mark_paint(in, marks[in.slot], coverage);
}

@fragment
fn fs_disc_coverage(in: MarkVarying) -> @location(0) vec4<f32> {
    let coverage = sdf_coverage(disc_distance(in));
    return mark_bin(in, coverage);
}
