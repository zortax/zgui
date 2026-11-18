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

@group(1) @binding(0) var shaded: texture_2d<u32>;

/// One `ShadedQuad`, which the arena pads to 7 texels.
fn load_shaded(slot: u32) -> ShadedQuad {
    let base = slot * 7u;
    let t0 = textureLoad(shaded, table_texel(base + 0u), 0);
    let t1 = textureLoad(shaded, table_texel(base + 1u), 0);
    let t2 = textureLoad(shaded, table_texel(base + 2u), 0);
    let t3 = textureLoad(shaded, table_texel(base + 3u), 0);
    let t4 = textureLoad(shaded, table_texel(base + 4u), 0);
    let t5 = textureLoad(shaded, table_texel(base + 5u), 0);
    let t6 = textureLoad(shaded, table_texel(base + 6u), 0);
    return ShadedQuad(
        t0.x,
        t0.y,
        Bounds(bitcast<f32>(t0.z), bitcast<f32>(t0.w), bitcast<f32>(t1.x), bitcast<f32>(t1.y)),
        Radii(bitcast<f32>(t1.z), bitcast<f32>(t1.w), bitcast<f32>(t2.x), bitcast<f32>(t2.y), bitcast<f32>(t2.z), bitcast<f32>(t2.w), bitcast<f32>(t3.x), bitcast<f32>(t3.y)),
        Edges(bitcast<f32>(t3.z), bitcast<f32>(t3.w), bitcast<f32>(t4.x), bitcast<f32>(t4.y)),
        PaintRef(t4.z, t4.w),
        PaintRef(t5.x, t5.y),
        t5.z,
        t5.w,
        Vector2(bitcast<f32>(t6.x), bitcast<f32>(t6.y)),
        t6.z,
        bitcast<f32>(t6.w),
    );
}

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
    // The point, from the quad's origin in the space it was encoded in.
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) instance: u32,
    // The quad's origin in that space.
    @location(2) @interpolate(flat) origin: vec2<f32>,
}

@vertex
fn vs_shaded(
    @builtin(vertex_index) vertex: u32,
    @location(0) slot: u32,
    @location(1) shift: vec2<f32>,
) -> ShadedVarying {
    let quad = load_shaded(slot);
    let local = inflated_corner(vertex, quad.bounds) + shift;
    var out: ShadedVarying;
    out.position = to_clip_position(local, quad.transform);
    out.local = inflated_offset(vertex, quad.bounds);
    out.instance = slot;
    out.origin = bounds_origin(quad.bounds);
    return out;
}

// What the effect is told about the point being drawn, at `local` from the quad's origin.
fn shader_input(quad: ShadedQuad, local: vec2<f32>) -> ShaderInput {
    let size = max(bounds_size(quad.bounds), vec2<f32>(1.0e-4));
    var input: ShaderInput;
    input.local = local;
    input.uv = input.local / size;
    input.size = size;
    input.scale = globals.frame.z;
    input.time = globals.frame.x;
    input.delta = globals.frame.y;
    input.pointer = shader_params.pointer;
    input.hovered = shader_params.hovered;
    return input;
}
