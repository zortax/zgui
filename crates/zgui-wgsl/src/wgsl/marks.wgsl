// Recognised shapes drawn from a shared payload: what every mark module shares.
//
// A mark item names its payload by count only. One draw call covers one payload kind of one item,
// and its instances walk that kind's payload range, so `instance_index` names the prim. The item
// itself is named by the draw's own block, through the remap and the chunk offsets every lane
// reads.
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
    let x = matrix[0].xy;
    let y = matrix[1].xy;
    let p = dot(x, x);
    let q = dot(y, y);
    let r = dot(x, y);
    let spread = sqrt(max(0.25 * (p - q) * (p - q) + r * r, 0.0));
    let smallest = sqrt(max(0.5 * (p + q) - spread, 1e-12));
    return min(1.0 / smallest, 1e4);
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

// The coverage of a signed distance in local units, antialiased over one device pixel.
//
// The derivatives turn the distance into device pixels along its own gradient, which is exact
// under any scale or turn. Call it in uniform control flow.
fn sdf_coverage(d: f32) -> f32 {
    let step = max(length(vec2<f32>(dpdx(d), dpdy(d))), 1e-6);
    return saturate(0.5 - d / step);
}

// The varying for one corner of a quad over `bounds` (x0 y0 x1 y1, payload space) of one prim.
fn mark_corner(
    vertex: u32,
    bounds: vec4<f32>,
    item: MarkItem,
    shift: vec2<f32>,
    coverage: bool,
) -> MarkVarying {
    let corner = unit_corner(vertex);
    let payload = bounds.xy + corner * (bounds.zw - bounds.xy);
    let local = payload + vec2<f32>(item.origin.x, item.origin.y) + shift;
    var out: MarkVarying;
    if coverage {
        out.position = to_page(local, item.transform);
    } else {
        out.position = to_target(local, item.transform, vec2<f32>(0.0));
    }
    out.local = local;
    out.shift = shift;
    out.caps = 0u;
    return out;
}

// Where a fragment's sample lies in payload space.
fn payload_point(in: MarkVarying, item: MarkItem) -> vec2<f32> {
    return in.local - in.shift - vec2<f32>(item.origin.x, item.origin.y);
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
