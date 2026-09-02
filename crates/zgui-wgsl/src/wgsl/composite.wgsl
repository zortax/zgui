// Compositing an isolated target back into the one beneath it.
//
// It is a textured quad and nothing more. The target it reads is premultiplied and gamma-encoded,
// exactly like every other target, so there is no colour conversion here — only a filter matrix
// where the content asked for one, the group's own opacity, and the clip chain every other
// pipeline applies through the same function.
//
// The sampler filters, because the target being read may be held at half resolution: an isolated
// target degrades in resolution rather than disappearing when the pool reaches its memory budget,
// and the blur chain's own output is half resolution by construction.
//
// Edge behaviour is a consequence of the geometry rather than a special case. A content filter's
// quad covers the region the filter *reads*, so a blur fades out past the element's box and the
// fade's shape comes from the group's own alpha against the transparent surround it was cleared
// to. A backdrop's quad covers only what it writes, so the frosted panel has the defined shape its
// clip gives it. Both then meet the same rounded clip.

struct CompositeParams {
    // The quad, in device pixels: origin then extent.
    bounds: vec4<f32>,
    // xy: the sampled target's extent in texels; z: how many of those one device pixel covers;
    // w: the multiplier on the whole composite's alpha.
    source: vec4<f32>,
    // x: the clip chain; y: the flags; zw: how far the sampled content is displaced, in device
    // pixels, which is what makes a drop shadow fall where it does.
    control: vec4<f32>,
    // The shadow's colour, premultiplied.
    tint: vec4<f32>,
    // The filter matrix, column by column.
    matrix0: vec4<f32>,
    matrix1: vec4<f32>,
    matrix2: vec4<f32>,
    matrix3: vec4<f32>,
    // Its constant term.
    matrix_offset: vec4<f32>,
}

@group(1) @binding(0) var<uniform> composite: CompositeParams;
@group(1) @binding(1) var isolated: texture_2d<f32>;
@group(1) @binding(2) var isolated_sampler: sampler;

// Replace what was sampled with a flat colour scaled by its alpha, which is a drop shadow.
const COMPOSITE_TINT: u32 = 1u;
// Run the sampled colour through the filter matrix.
const COMPOSITE_MATRIX: u32 = 2u;

struct CompositeVarying {
    @builtin(position) position: vec4<f32>,
    // `device` is a reserved word in Metal, and member names reach the MSL output verbatim.
    @location(0) device_point: vec2<f32>,
}

@vertex
fn vs_composite(@builtin(vertex_index) vertex: u32) -> CompositeVarying {
    let corner = unit_corner(vertex);
    let device = composite.bounds.xy + corner * composite.bounds.zw;
    var out: CompositeVarying;
    out.position = to_clip_position(device, 0u);
    out.device_point = device;
    return out;
}

@fragment
fn fs_composite(in: CompositeVarying) -> @location(0) vec4<f32> {
    let coverage = clip_coverage(in.device_point, u32(composite.control.x));
    if coverage <= 0.0 {
        return vec4<f32>(0.0);
    }
    let flags = u32(composite.control.y);
    let sampled = sample_isolated(in.device_point - composite.control.zw);

    var color = sampled;
    if (flags & COMPOSITE_TINT) != 0u {
        // A shadow keeps only the coverage of what cast it.
        color = composite.tint * sampled.a;
    } else if (flags & COMPOSITE_MATRIX) != 0u {
        color = apply_filter_matrix(sampled);
    }
    return color * coverage * composite.source.w;
}

// The isolated target at a device position, through the same grid it was written on.
fn sample_isolated(device: vec2<f32>) -> vec4<f32> {
    let uv = device * composite.source.z / composite.source.xy;
    return textureSampleLevel(isolated, isolated_sampler, uv, 0.0);
}

// The filter matrix, applied to unpremultiplied colour.
//
// CSS filter functions are defined on colour that is not scaled by its alpha, so a half-covered
// edge texel would otherwise be brightened or inverted as though it were half dark rather than
// half absent — which shows as a ring around everything a filter touches.
fn apply_filter_matrix(premultiplied: vec4<f32>) -> vec4<f32> {
    let alpha = premultiplied.a;
    if alpha <= 0.0 {
        return vec4<f32>(0.0);
    }
    let straight = vec4<f32>(premultiplied.rgb / alpha, alpha);
    let filtered = clamp(
        composite.matrix0 * straight.r
            + composite.matrix1 * straight.g
            + composite.matrix2 * straight.b
            + composite.matrix3 * straight.a
            + composite.matrix_offset,
        vec4<f32>(0.0),
        vec4<f32>(1.0),
    );
    return vec4<f32>(filtered.rgb * filtered.a, filtered.a);
}
