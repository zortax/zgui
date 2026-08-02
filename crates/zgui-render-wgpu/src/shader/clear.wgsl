// Clearing one damage rectangle.
//
// A render pass clears its whole attachment or none of it: there is no scissored clear operation.
// So a rectangle that is about to be redrawn is cleared by drawing over it — one full-target
// triangle strip with the scissor set to the rectangle, writing transparent with no blend, which
// replaces every texel the scissor admits and touches nothing else.
//
// It has no bindings at all. What is cleared is the scissor's business, and the scissor is set by
// the pass rather than read by the shader.

struct ClearVarying {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_clear(@builtin(vertex_index) vertex: u32) -> ClearVarying {
    let corner = vec2<f32>(f32(vertex & 1u), 0.5 * f32(vertex & 2u));
    var out: ClearVarying;
    out.position = vec4<f32>(corner.x * 2.0 - 1.0, 1.0 - corner.y * 2.0, 0.0, 1.0);
    return out;
}

@fragment
fn fs_clear(in: ClearVarying) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0);
}
