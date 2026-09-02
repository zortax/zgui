// The framework half of a filter effect: the block, the stage over the region, and the call.
//
// A filter is not a rectangle. Its content is already in a target of its own — the same target a
// `blur()` or a `backdrop-filter` costs — and this pass reads that target and writes what replaces
// it. The scissor is what restricts it to the region being filtered, so a filtered panel costs its
// own area rather than the window's.

struct FilterParams {
    // xy: the source's extent in texels; zw: the destination's.
    extents: vec4<f32>,
    // xy: the device position of the element's own origin; z: texels of the source per device
    // pixel; w: texels of the destination per device pixel.
    placement: vec4<f32>,
    // The part of the source that was actually written, in source texels: the two corners.
    valid: vec4<f32>,
    // xy: the element's extent in device pixels; z: seconds since the document started;
    // w: seconds the previous frame took.
    element: vec4<f32>,
    // x: device pixels per CSS pixel; yzw unused.
    //
    // A filter binds none of the frame's own tables — the content it reads is at group zero — so
    // what it knows about the frame arrives in this block rather than in the shared globals.
    frame: vec4<f32>,
}

@group(0) @binding(0) var<uniform> filtered: FilterParams;
@group(0) @binding(1) var filter_source: texture_2d<f32>;
@group(0) @binding(2) var filter_sampler: sampler;

struct ShaderParams {
    // The pointer, in the element's own device pixels.
    pointer: vec2<f32>,
    // One while the pointer is over the element.
    hovered: f32,
    // Unused, and present so the application's half begins on a sixteen-byte boundary.
    reserved: f32,
    user: Params,
}

@group(1) @binding(0) var<uniform> shader_params: ShaderParams;

struct FilterVarying {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_shaded_filter(@builtin(vertex_index) vertex: u32) -> FilterVarying {
    let corner = vec2<f32>(f32(vertex & 1u), 0.5 * f32(vertex & 2u));
    var out: FilterVarying;
    out.position = vec4<f32>(corner.x * 2.0 - 1.0, 1.0 - corner.y * 2.0, 0.0, 1.0);
    return out;
}

@fragment
fn fs_shaded_filter(in: FilterVarying) -> @location(0) vec4<f32> {
    // The device pixel this destination texel's centre sits at, then the same point in the
    // element's own coordinates, which is what the effect is written against.
    let device = (floor(in.position.xy) + vec2<f32>(0.5)) / filtered.placement.w;
    let local = device - filtered.placement.xy;

    var input: ShaderInput;
    input.local = local;
    input.size = max(filtered.element.xy, vec2<f32>(1.0e-4));
    input.uv = input.local / input.size;
    input.scale = filtered.frame.x;
    input.time = filtered.element.z;
    input.delta = filtered.element.w;
    input.pointer = shader_params.pointer;
    input.hovered = shader_params.hovered;

    var region: FilterSource;
    region.origin = filtered.placement.xy;
    region.scale = filtered.placement.z;
    region.extent = filtered.extents.xy;
    region.low = filtered.valid.xy;
    region.high = filtered.valid.zw;

    let color = apply(input, shader_params.user, filter_source, filter_sampler, region);
    // A filtering pass replaces what was there, so a value outside the unit range would reach the
    // screen exactly as written.
    return clamp(color, vec4<f32>(0.0), vec4<f32>(1.0));
}
