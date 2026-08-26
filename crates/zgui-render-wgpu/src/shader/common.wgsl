// The vocabulary every pipeline shares: the frame's globals, the side tables addressed by index
// from every instance, and the plain-old-data spellings of the display list's structures.
//
// Every field of every instance structure is a four-byte scalar, and that is deliberate rather
// than clumsy: a `vec4<f32>` would carry a sixteen-byte alignment that the packed Rust structures
// do not have, so the two layouts would agree on some fields and silently disagree on others. The
// shader-reflection check compares these declarations against the Rust ones field by field.

struct Globals {
    // xy: the target's extent in texels.
    // zw: how many texels one device pixel covers — one in the composed target, a half in a
    //     half-resolution isolated one. Every primitive is positioned and clipped in device
    //     pixels whichever target it lands in, and this is the whole of the difference.
    viewport: vec4<f32>,
    // The four gamma-correction coefficients coverage is corrected with.
    gamma_ratios: vec4<f32>,
    // x: contrast enhancement for single-channel coverage.
    // y: contrast enhancement for per-channel coverage.
    // z: non-zero when the display's subpixels run blue to red.
    text: vec4<f32>,
}

// A box on the device pixel grid: origin then extent.
struct Bounds {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

// Per-corner elliptical radii, clockwise from the top left.
struct Radii {
    tl_x: f32,
    tl_y: f32,
    tr_x: f32,
    tr_y: f32,
    br_x: f32,
    br_y: f32,
    bl_x: f32,
    bl_y: f32,
}

// Premultiplied, gamma-encoded sRGB.
struct Rgba {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

// Widths of the four sides.
struct Edges {
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
}

// A primitive's reference to its paint: a family and an index.
struct PaintRef {
    kind: u32,
    index: u32,
}

// Four floats whose meaning is whatever reads them.
struct Vector4 {
    x: f32,
    y: f32,
    z: f32,
    w: f32,
}

// Three floats, used to keep a structure free of padding.
struct Vector3 {
    x: f32,
    y: f32,
    z: f32,
}

// Two floats: a point or a displacement, spelled as scalars so it carries no wider alignment than
// the packed structures it is a member of.
struct Vector2 {
    x: f32,
    y: f32,
}

// A rectangle of an atlas texture, in texels.
struct TileRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

// Where a cached raster lives.
struct Tile {
    texture: u32,
    tile: u32,
    bounds: TileRect,
}

// One rounded-rectangle test of a clip chain.
struct Rounded {
    rect: Bounds,
    radii: Radii,
}

// A whole clip chain, flattened into what one draw call applies.
struct Clip {
    aabb: Bounds,
    first: Rounded,
    second: Rounded,
    count: u32,
    has_mask: u32,
    mask: Tile,
}

// One paint source.
struct Paint {
    // 0 nothing, 1 solid, 2 gradient, 3 image.
    kind: u32,
    // 0 linear, 1 radial, 2 conic.
    gradient: u32,
    // Which space the ramp's stops were written in: 0 encoded sRGB, 1 Oklab, 2 linear sRGB.
    space: u32,
    // 1 when the ramp repeats outside its extent.
    flags: u32,
    // Linear: start then end. Radial: centre then the two radii. Conic: centre, then start angle.
    geometry: Vector4,
    // The colour of a solid paint.
    color: Rgba,
    stop_start: u32,
    stop_count: u32,
    pad0: u32,
    pad1: u32,
}

// One stop of a ramp, in the space the ramp is interpolated in, alpha-premultiplied.
struct Stop {
    color: Vector4,
    offset: f32,
    pad: Vector3,
}

// One coordinate system: the matrix mapping it onto the device.
struct Spatial {
    matrix: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> clips: array<Clip>;
@group(0) @binding(2) var<storage, read> paints: array<Paint>;
@group(0) @binding(3) var<storage, read> stops: array<Stop>;
@group(0) @binding(4) var<storage, read> spatial: array<Spatial>;

const PAINT_NONE: u32 = 0u;
const PAINT_SOLID: u32 = 1u;
const PAINT_GRADIENT: u32 = 2u;
const PAINT_IMAGE: u32 = 3u;

const SPACE_SRGB: u32 = 0u;
const SPACE_OKLAB: u32 = 1u;
const SPACE_LINEAR_SRGB: u32 = 2u;

const M_PI: f32 = 3.141592653589793;

fn bounds_origin(b: Bounds) -> vec2<f32> {
    return vec2<f32>(b.x, b.y);
}

fn bounds_size(b: Bounds) -> vec2<f32> {
    return vec2<f32>(b.w, b.h);
}

fn rgba_of(c: Rgba) -> vec4<f32> {
    return vec4<f32>(c.r, c.g, c.b, c.a);
}

fn tile_bounds(t: Tile) -> vec4<f32> {
    return vec4<f32>(f32(t.bounds.x), f32(t.bounds.y), f32(t.bounds.w), f32(t.bounds.h));
}

fn vector4_of(v: Vector4) -> vec4<f32> {
    return vec4<f32>(v.x, v.y, v.z, v.w);
}

// The four corners of the unit square, in triangle-strip order.
fn unit_corner(vertex: u32) -> vec2<f32> {
    return vec2<f32>(f32(vertex & 1u), 0.5 * f32(vertex & 2u));
}

// A device-space point, transformed and projected into the current target's clip space.
fn to_clip_position(point: vec2<f32>, spatial_id: u32) -> vec4<f32> {
    let world = spatial[spatial_id].matrix * vec4<f32>(point, 0.0, 1.0);
    let texel = world.xy * globals.viewport.zw;
    let ndc = vec2<f32>(
        texel.x / globals.viewport.x * 2.0 - world.w,
        world.w - texel.y / globals.viewport.y * 2.0,
    );
    return vec4<f32>(ndc, 0.0, world.w);
}

// The device pixel a fragment's own target coordinate names.
//
// Shapes and clips are in device pixels in every target, so a half-resolution target evaluates
// exactly the same geometry as the composed one — at half the sample rate, which is the entire
// difference between the two and the only one there should be.
fn device_position(position: vec2<f32>) -> vec2<f32> {
    return position / globals.viewport.zw;
}

// The device-space point a unit-square corner maps to, with one pixel of slack on every side so an
// antialiased edge has somewhere to land.
fn inflated_corner(vertex: u32, b: Bounds) -> vec2<f32> {
    let origin = bounds_origin(b) - vec2<f32>(1.0);
    let size = bounds_size(b) + vec2<f32>(2.0);
    return origin + unit_corner(vertex) * size;
}

// A resolved remap entry: the arena slot in the low bits, and above them the index of the frame
// chunk offset the instance is drawn shifted by. The offsets are what let a chunk that merely
// moved keep its resident bytes — the geometry shifts here, in the vertex stage, and a fragment
// stage comparing against encode-space fields subtracts the same shift from its sample point.
// The widths mirror SLOT_BITS in buffer/persist.rs.
const REMAP_SLOT_MASK: u32 = 0xFFFFFFu;
const REMAP_OFFSET_SHIFT: u32 = 24u;
