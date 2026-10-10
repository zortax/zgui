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

// The coverage of the box `rect` (x0 y0 x1 y1) with `radii` and corner `shape` at `point`.
//
// Along a straight side the coverage is the product of the coverage across each axis, which is
// the exact area of the pixel inside a square corner. Inside a rounded corner it is the coverage
// of the corner's own distance.
fn box_cover(
    point: vec2<f32>,
    rect: vec4<f32>,
    radii: Radii,
    shape: f32,
    across: vec2<f32>,
    down: vec2<f32>,
) -> f32 {
    let half = 0.5 * (rect.zw - rect.xy);
    let centre_to_point = point - 0.5 * (rect.xy + rect.zw);
    let flip = select(vec2<f32>(-1.0), vec2<f32>(1.0), centre_to_point >= vec2<f32>(0.0));
    let r = pick_corner_radii(centre_to_point, radii);
    let corner = abs(centre_to_point) - half;
    let from_centre = corner + r;
    if from_centre.x > 0.0 && from_centre.y > 0.0 && r.x > 0.0 && r.y > 0.0 {
        var rounded: Distance;
        rounded.d = quad_sdf_impl(from_centre, r, shape);
        if shape == CORNER_ROUND && r.x == r.y {
            rounded.gradient = normalize(from_centre) * flip;
        } else {
            let step = 0.01 * max(max(length(across), length(down)), 1e-6);
            rounded.gradient = vec2<f32>(
                quad_sdf_impl(from_centre + vec2<f32>(step, 0.0), r, shape) - rounded.d,
                quad_sdf_impl(from_centre + vec2<f32>(0.0, step), r, shape) - rounded.d,
            ) / step * flip;
        }
        return sdf_coverage(rounded, across, down);
    }
    var x: Distance;
    x.d = corner.x;
    x.gradient = vec2<f32>(flip.x, 0.0);
    var y: Distance;
    y.d = corner.y;
    y.gradient = vec2<f32>(0.0, flip.y);
    return sdf_coverage(x, across, down) * sdf_coverage(y, across, down);
}

// The coverage of one payload box: its area, less the area inside its border when it has one.
fn box_coverage(in: MarkVarying, across: vec2<f32>, down: vec2<f32>) -> f32 {
    let item = marks[in.slot];
    let prim = mark_boxes[in.prim];
    let point = payload_point(in, item);
    let rect = vector4_of(prim.rect);
    let outer = box_cover(point, rect, prim.radii, prim.shape.x, across, down);
    let border = prim.shape.y;
    if border <= 0.0 {
        return outer;
    }
    let inner_rect = vec4<f32>(rect.xy + vec2<f32>(border), max(rect.zw - vec2<f32>(border), rect.xy + vec2<f32>(border)));
    var inner_radii: Radii;
    inner_radii.tl_x = max(prim.radii.tl_x - border, 0.0);
    inner_radii.tl_y = max(prim.radii.tl_y - border, 0.0);
    inner_radii.tr_x = max(prim.radii.tr_x - border, 0.0);
    inner_radii.tr_y = max(prim.radii.tr_y - border, 0.0);
    inner_radii.br_x = max(prim.radii.br_x - border, 0.0);
    inner_radii.br_y = max(prim.radii.br_y - border, 0.0);
    inner_radii.bl_x = max(prim.radii.bl_x - border, 0.0);
    inner_radii.bl_y = max(prim.radii.bl_y - border, 0.0);
    let inner = box_cover(point, inner_rect, inner_radii, prim.shape.x, across, down);
    return saturate(outer - inner);
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
    let across = dpdx(in.local);
    let down = dpdy(in.local);
    return mark_paint(in, marks[in.slot], box_coverage(in, across, down));
}

@fragment
fn fs_box_coverage(in: MarkVarying) -> @location(0) vec4<f32> {
    let across = dpdx(in.local);
    let down = dpdy(in.local);
    return mark_bin(in, box_coverage(in, across, down));
}
