// Rectangles, rounded rectangles and their borders in a mark payload.

struct MarkBox {
    // x0 y0 x1 y1.
    rect: Vector4,
    radii: Radii,
    // x: the superellipse exponent. y: the border width, zero for a filled box.
    shape: Vector4,
}

@group(2) @binding(1) var<storage, read> mark_boxes: array<MarkBox>;

fn box_corner(vertex: u32, instance: u32, coverage: bool) -> MarkVarying {
    let found = item_of(mark_draw.position);
    let item = marks[found.slot];
    let prim = mark_boxes[instance];
    let margin = vec2<f32>(margin_local(item.transform));
    let bounds = vec4<f32>(
        vec2<f32>(prim.rect.x, prim.rect.y) - margin,
        vec2<f32>(prim.rect.z, prim.rect.w) + margin,
    );
    var out = mark_corner(vertex, bounds, item, found.shift, coverage);
    out.slot = found.slot;
    out.prim = instance;
    return out;
}

// Signed distance to the box, or to its border when it has one, negative inside.
fn box_distance(in: MarkVarying) -> f32 {
    let item = marks[in.slot];
    let prim = mark_boxes[in.prim];
    let point = payload_point(in, item);
    var outer_bounds: Bounds;
    outer_bounds.x = prim.rect.x;
    outer_bounds.y = prim.rect.y;
    outer_bounds.w = prim.rect.z - prim.rect.x;
    outer_bounds.h = prim.rect.w - prim.rect.y;
    let outer = quad_sdf(point, outer_bounds, prim.radii, prim.shape.x);
    let border = prim.shape.y;
    var inner_bounds: Bounds;
    inner_bounds.x = outer_bounds.x + border;
    inner_bounds.y = outer_bounds.y + border;
    inner_bounds.w = max(outer_bounds.w - 2.0 * border, 0.0);
    inner_bounds.h = max(outer_bounds.h - 2.0 * border, 0.0);
    var inner_radii: Radii;
    inner_radii.tl_x = max(prim.radii.tl_x - border, 0.0);
    inner_radii.tl_y = max(prim.radii.tl_y - border, 0.0);
    inner_radii.tr_x = max(prim.radii.tr_x - border, 0.0);
    inner_radii.tr_y = max(prim.radii.tr_y - border, 0.0);
    inner_radii.br_x = max(prim.radii.br_x - border, 0.0);
    inner_radii.br_y = max(prim.radii.br_y - border, 0.0);
    inner_radii.bl_x = max(prim.radii.bl_x - border, 0.0);
    inner_radii.bl_y = max(prim.radii.bl_y - border, 0.0);
    let inner = quad_sdf(point, inner_bounds, inner_radii, prim.shape.x);
    // Both are measured for every box, so the result reaches the derivatives the same way for a
    // filled box and a bordered one.
    return select(outer, max(outer, -inner), border > 0.0);
}

@vertex
fn vs_box_paint(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32,
) -> MarkVarying {
    return box_corner(vertex, instance, false);
}

@vertex
fn vs_box_coverage(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32,
) -> MarkVarying {
    return box_corner(vertex, instance, true);
}

@fragment
fn fs_box_paint(in: MarkVarying) -> @location(0) vec4<f32> {
    let coverage = sdf_coverage(box_distance(in));
    return mark_paint(in, marks[in.slot], coverage);
}

@fragment
fn fs_box_coverage(in: MarkVarying) -> @location(0) vec4<f32> {
    let coverage = sdf_coverage(box_distance(in));
    return mark_bin(in, coverage);
}
