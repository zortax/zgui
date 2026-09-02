// The framework half of an application effect that draws a rectangle: the instance, the parameter
// block, the vertex stage, and the input an effect is called with.
//
// The instance is a quad with four more words. It goes through the same arena, the same draw-order
// permutation and the same chunk offsets as an ordinary quad, so an effect that merely moved keeps
// its resident bytes exactly as a background does.

struct ShadedQuad {
    order: u32,
    // Which registered effect draws this.
    shader: u32,
    bounds: Bounds,
    radii: Radii,
    border: Edges,
    // Read by a coverage effect, which shades no colour of its own. Nothing for a paint effect.
    fill: PaintRef,
    stroke: PaintRef,
    clip: u32,
    transform: u32,
    // Where the space the two paints were resolved in has its origin, as an ordinary quad carries.
    paint_origin: Vector2,
    // The parameter slot this draws with. Read on the host to break batches, and not read here:
    // the block is bound beside the draw, because every instance of one draw shares it.
    params: u32,
    // The alpha folded in from the groups above.
    opacity: f32,
}

@group(1) @binding(0) var<storage, read> shaded: array<ShadedQuad>;
@group(1) @binding(1) var<storage, read> remap: array<u32>;
@group(1) @binding(2) var<storage, read> chunk_offsets: array<vec2<f32>>;

// One effect's parameters: the framework's half, then the application's own structure.
//
// `Params` is declared by the application, above this. Its layout is compared against the Rust
// structure the host writes, the same way every instance structure is.
struct ShaderParams {
    // The pointer, in the element's own device pixels.
    pointer: vec2<f32>,
    // One while the pointer is over the element.
    hovered: f32,
    // Unused, and present so the application's half begins on a sixteen-byte boundary.
    reserved: f32,
    user: Params,
}

@group(2) @binding(0) var<uniform> shader_params: ShaderParams;

struct ShadedVarying {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) instance: u32,
    @location(2) @interpolate(flat) shift: vec2<f32>,
}

@vertex
fn vs_shaded(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32,
) -> ShadedVarying {
    let packed = remap[instance];
    let slot = packed & REMAP_SLOT_MASK;
    let shift = chunk_offsets[packed >> REMAP_OFFSET_SHIFT];
    let quad = shaded[slot];
    let local = inflated_corner(vertex, quad.bounds) + shift;
    var out: ShadedVarying;
    out.position = to_clip_position(local, quad.transform);
    out.local = local;
    out.instance = slot;
    out.shift = shift;
    return out;
}

// What the effect is told about the point being drawn.
fn shader_input(quad: ShadedQuad, point: vec2<f32>) -> ShaderInput {
    let origin = bounds_origin(quad.bounds);
    let size = max(bounds_size(quad.bounds), vec2<f32>(1.0e-4));
    var input: ShaderInput;
    input.local = point - origin;
    input.uv = input.local / size;
    input.size = size;
    input.scale = globals.frame.z;
    input.time = globals.frame.x;
    input.delta = globals.frame.y;
    input.pointer = shader_params.pointer;
    input.hovered = shader_params.hovered;
    return input;
}
