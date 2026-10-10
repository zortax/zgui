// Recognised shapes drawn from a shared payload: what every mark module shares.
//
// A mark item names its payload by count only. One draw call covers one payload kind of one item,
// and its instances walk that kind's payload range, so `instance_index` names the prim. The item
// itself is named by the draw's own block, through the remap and the chunk offsets every lane
// reads.
//
// A payload maps to the item's local space by the item's axes and origin. Distances are measured in
// a *distance space*: payload units without the screen flag, so every length scales with the
// axes, and local units with it, so a marker keeps its size under any axes.
//
// A draw paints in one of two ways. A *paint* draw writes paint times coverage times clip into the
// target, which is exact when the prims are apart. A *coverage* draw adds the coverage alone into a
// single-channel bin, and one composite later paints the item through the sum: the union of
// overlapping prims, painted once.

struct MarkItem {
    order: u32,
    flags: u32,
    bounds: Bounds,
    paint: PaintRef,
    clip: u32,
    transform: u32,
    // Where the space the paint was resolved in has its origin.
    paint_origin: Vector2,
    // Added to every payload coordinate. A replay moves this and leaves the payload as it is.
    origin: Vector2,
    discs: u32,
    boxes: u32,
    vertices: u32,
    half_width: f32,
    // The linear part of the payload-to-local map: x' = a x + c y, y' = b x + d y.
    axes: Vector4,
}

// What one draw of one item reads beside the payload.
//
// Plain vectors rather than the scalar spellings the instance structures use, because a uniform
// block aligns a nested structure to sixteen bytes.
struct MarkDraw {
    // The item's position in the draw-order remap.
    position: u32,
    // The bin page a coverage draw writes and a composite reads.
    page: u32,
    // Added to a device point to find its texel in the bin page.
    shift: vec2<f32>,
    // The device pixels the bin holds: origin then extent.
    region: vec4<f32>,
}

@group(1) @binding(0) var<storage, read> marks: array<MarkItem>;
@group(1) @binding(1) var<storage, read> remap: array<u32>;
@group(1) @binding(2) var<storage, read> chunk_offsets: array<vec2<f32>>;
@group(2) @binding(0) var<uniform> mark_draw: MarkDraw;

const MARK_UNION: u32 = 1u;
const MARK_SCREEN: u32 = 2u;
const MARK_SQUARE_DISCS: u32 = 4u;
const MARK_START_CAP_SHIFT: u32 = 8u;
const MARK_END_CAP_SHIFT: u32 = 10u;
const CAP_BUTT: u32 = 0u;
const CAP_SQUARE: u32 = 1u;
const CAP_ROUND: u32 = 2u;

struct MarkVarying {
    @builtin(position) position: vec4<f32>,
    // The point in the item's own space, with the chunk offset added.
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) slot: u32,
    @location(2) @interpolate(flat) shift: vec2<f32>,
    // The prim's index in its payload kind.
    @location(3) @interpolate(flat) prim: u32,
    // The polyline caps: start in bits 0 and 1, end in bits 2 and 3.
    @location(4) @interpolate(flat) caps: u32,
    // The point in distance space, with no chunk offset.
    @location(5) point: vec2<f32>,
}

// The item a draw names: its arena slot, and the chunk offset it is drawn shifted by.
struct MarkRef {
    slot: u32,
    shift: vec2<f32>,
}

fn item_of(position: u32) -> MarkRef {
    let packed = remap[position];
    var found: MarkRef;
    found.slot = packed & REMAP_SLOT_MASK;
    found.shift = chunk_offsets[packed >> REMAP_OFFSET_SHIFT];
    return found;
}

// The length in local units of the shortest device pixel: one over the smallest singular value of
// the transform's linear part. A quad grown by this much in every direction holds the whole
// antialiased edge under any scale or turn.
fn margin_local(transform: u32) -> f32 {
    let matrix = spatial[transform].matrix;
    return margin_of(matrix[0].xy, matrix[1].xy);
}

// One over the smallest singular value of the matrix with columns `x` and `y`, at most 1e4.
fn margin_of(x: vec2<f32>, y: vec2<f32>) -> f32 {
    let p = dot(x, x);
    let q = dot(y, y);
    let r = dot(x, y);
    let spread = sqrt(max(0.25 * (p - q) * (p - q) + r * r, 0.0));
    let smallest = sqrt(max(0.5 * (p + q) - spread, 1e-12));
    return min(1.0 / smallest, 1e4);
}

// The item's axes as a matrix.
fn mark_axes(item: MarkItem) -> mat2x2<f32> {
    return mat2x2<f32>(item.axes.x, item.axes.y, item.axes.z, item.axes.w);
}

fn is_screen(item: MarkItem) -> bool {
    return (item.flags & MARK_SCREEN) != 0u;
}

// A payload position in distance space.
fn to_space(item: MarkItem, p: vec2<f32>) -> vec2<f32> {
    if is_screen(item) {
        return mark_axes(item) * p + vec2<f32>(item.origin.x, item.origin.y);
    }
    return p;
}

// A point in distance space, in the item's local space.
fn to_local(item: MarkItem, q: vec2<f32>) -> vec2<f32> {
    if is_screen(item) {
        return q;
    }
    return mark_axes(item) * q + vec2<f32>(item.origin.x, item.origin.y);
}

// The length in distance units of the shortest device pixel.
fn margin_space(item: MarkItem) -> f32 {
    if is_screen(item) {
        return margin_local(item.transform);
    }
    let matrix = spatial[item.transform].matrix;
    let linear = mat2x2<f32>(matrix[0].xy, matrix[1].xy) * mark_axes(item);
    return margin_of(linear[0], linear[1]);
}

// A point in the item's space, on the device and moved by `shift`, in the target's clip space.
fn to_target(local: vec2<f32>, transform: u32, shift: vec2<f32>) -> vec4<f32> {
    let world = spatial[transform].matrix * vec4<f32>(local, 0.0, 1.0);
    let device = world.xy / world.w + shift;
    let texel = device * globals.viewport.zw;
    return vec4<f32>(
        texel.x / globals.viewport.x * 2.0 - 1.0,
        1.0 - texel.y / globals.viewport.y * 2.0,
        0.0,
        1.0,
    );
}

// A point in the item's space, in the clip space of the bin page this draw writes.
fn to_page(local: vec2<f32>, transform: u32) -> vec4<f32> {
    return to_target(local, transform, mark_draw.shift);
}

// A signed distance in payload units, negative inside, and its gradient.
struct Distance {
    d: f32,
    gradient: vec2<f32>,
    // The width of the shape behind the edge, against the gradient: a band, such as a ring or a
    // stroke, ends at a far edge this far from the near one. Zero for a shape with no far edge.
    band: f32,
}

// The coverage of `distance`: the area of the pixel inside the edge.
//
// `across` and `down` are how far one device pixel moves the payload point, the derivatives of
// the interpolated point, which are exact because the point is affine on the screen. The gradient
// through them is the edge's normal on the screen and how far one pixel moves the distance, so
// the coverage is right under any scale or turn. A gradient taken from the derivatives of the
// distance itself would cancel across the ridge of a thin ring or a thin stroke and paint it
// solid.
//
// A band thinner than a pixel can end inside the pixel on both sides. The area past its far edge
// is then removed, so a hairline covers its own width. A band wider than about one and a half
// pixels removes nothing.
fn sdf_coverage(distance: Distance, across: vec2<f32>, down: vec2<f32>) -> f32 {
    let normal = vec2<f32>(dot(distance.gradient, across), dot(distance.gradient, down));
    let step = max(length(normal), 1e-6);
    let along = abs(normal) / step;
    let a = max(along.x, along.y);
    let b = min(along.x, along.y);
    let inside = -distance.d / step;
    let near = edge_coverage(inside, a, b);
    if distance.band <= 0.0 {
        return near;
    }
    return max(near - edge_coverage(inside - distance.band / step, a, b), 0.0);
}

// The area of a unit pixel on the inner side of a straight edge `inside` pixels from its centre,
// for an edge whose unit normal has the components `a` and `b`, `a` the larger.
//
// The projection of the pixel onto the normal is spread like the sum of two uniform variables of
// widths `a` and `b`: a trapezoid, flat over `a − b` and sloped over `b` at each end. Its area up to
// `inside` is the coverage, exact for a straight edge at any angle.
fn edge_coverage(inside: f32, a: f32, b: f32) -> f32 {
    let outer = 0.5 * (a + b);
    let inner = 0.5 * (a - b);
    if inside <= -outer {
        return 0.0;
    }
    if inside >= outer {
        return 1.0;
    }
    if b < 1e-4 {
        return saturate(0.5 + inside / a);
    }
    if inside < -inner {
        let t = inside + outer;
        return t * t / (2.0 * a * b);
    }
    if inside > inner {
        let t = outer - inside;
        return 1.0 - t * t / (2.0 * a * b);
    }
    return 0.5 + inside / a;
}

// The larger of two distances, with the gradient of that one.
fn distance_max(one: Distance, two: Distance) -> Distance {
    if two.d > one.d {
        return two;
    }
    return one;
}

// The smaller of two distances, with the gradient of that one.
fn distance_min(one: Distance, two: Distance) -> Distance {
    if two.d < one.d {
        return two;
    }
    return one;
}

// The distance from a disc of `radius` around `centre`, outward from it.
fn radial(point: vec2<f32>, centre: vec2<f32>, radius: f32) -> Distance {
    let along = point - centre;
    let length_along = length(along);
    var out: Distance;
    out.d = length_along - radius;
    out.gradient = select(vec2<f32>(1.0, 0.0), along / length_along, length_along > 0.0);
    out.band = 0.0;
    return out;
}

// The varying for the point `q` in distance space of one prim.
fn mark_point(q: vec2<f32>, item: MarkItem, shift: vec2<f32>, coverage: bool) -> MarkVarying {
    let local = to_local(item, q) + shift;
    var out: MarkVarying;
    if coverage {
        out.position = to_page(local, item.transform);
    } else {
        out.position = to_target(local, item.transform, vec2<f32>(0.0));
    }
    out.local = local;
    out.shift = shift;
    out.caps = 0u;
    out.point = q;
    return out;
}

// The varying for one corner of a quad over `bounds` (x0 y0 x1 y1, distance space) of one prim.
fn mark_corner(
    vertex: u32,
    bounds: vec4<f32>,
    item: MarkItem,
    shift: vec2<f32>,
    coverage: bool,
) -> MarkVarying {
    let corner = unit_corner(vertex);
    return mark_point(bounds.xy + corner * (bounds.zw - bounds.xy), item, shift, coverage);
}

// Where a fragment's sample lies in distance space.
fn payload_point(in: MarkVarying) -> vec2<f32> {
    return in.point;
}

// The premultiplied colour a paint draw writes for `coverage`.
fn mark_paint(in: MarkVarying, item: MarkItem, coverage: f32) -> vec4<f32> {
    let clip = clip_coverage(device_position(in.position.xy), item.clip);
    if clip <= 0.0 || coverage <= 0.0 {
        return vec4<f32>(0.0);
    }
    let point = in.local - in.shift;
    let origin = vec2<f32>(item.paint_origin.x, item.paint_origin.y);
    return paint_color(item.paint, point, origin) * coverage * clip;
}

// What a coverage draw adds into its bin: nothing outside the bin's region.
fn mark_bin(in: MarkVarying, coverage: f32) -> vec4<f32> {
    let device = in.position.xy - mark_draw.shift;
    let region = mark_draw.region;
    let inside = device.x >= region.x && device.y >= region.y
        && device.x < region.x + region.z && device.y < region.y + region.w;
    return vec4<f32>(select(0.0, coverage, inside), 0.0, 0.0, 0.0);
}

// Whether a payload vertex is a run separator.
fn is_separator(vertex: vec2<f32>) -> bool {
    let bits = bitcast<vec2<u32>>(vertex) & vec2<u32>(0x7fffffffu);
    return bits.x > 0x7f800000u || bits.y > 0x7f800000u;
}
