// Painting one union item through the coverage its prims summed into its bin.
//
// The bin holds coverage in device pixels, at one texel each, so the read is a `textureLoad` with
// no sampler. A half-resolution target covers two by two device pixels with each texel, and reads
// the mean of the four: an exact box filter of the full-resolution coverage.

@group(2) @binding(1) var mark_bins: texture_2d_array<f32>;

@vertex
fn vs_mark_composite(@builtin(vertex_index) vertex: u32) -> MarkVarying {
    let found = item_of(mark_draw.position);
    let item = marks[found.slot];
    let margin = vec2<f32>(margin_local(item.transform));
    let origin = bounds_origin(item.bounds) - margin;
    let size = bounds_size(item.bounds) + 2.0 * margin;
    let local = origin + unit_corner(vertex) * size + found.shift;
    var out: MarkVarying;
    out.position = to_target(local, item.transform, vec2<f32>(0.0));
    out.local = local;
    out.slot = found.slot;
    out.shift = found.shift;
    out.prim = 0u;
    out.caps = 0u;
    return out;
}

// The summed coverage of one device pixel, or nothing outside the bin.
fn bin_coverage(device: vec2<f32>) -> f32 {
    let region = mark_draw.region;
    if device.x < region.x || device.y < region.y
        || device.x >= region.x + region.z || device.y >= region.y + region.w {
        return 0.0;
    }
    let texel = vec2<i32>(floor(device + mark_draw.shift));
    return textureLoad(mark_bins, texel, i32(mark_draw.page), 0).r;
}

@fragment
fn fs_mark_composite(in: MarkVarying) -> @location(0) vec4<f32> {
    let item = marks[in.slot];
    let device = device_position(in.position.xy);
    var coverage = 0.0;
    if globals.viewport.z >= 1.0 {
        coverage = bin_coverage(floor(device) + vec2<f32>(0.5));
    } else {
        // The four device pixels whose centres this texel's footprint holds.
        let first = floor(device - vec2<f32>(0.5)) + vec2<f32>(0.5);
        coverage = 0.25 * (bin_coverage(first) + bin_coverage(first + vec2<f32>(1.0, 0.0))
            + bin_coverage(first + vec2<f32>(0.0, 1.0)) + bin_coverage(first + vec2<f32>(1.0, 1.0)));
    }
    let clip = clip_coverage(device, item.clip);
    if clip <= 0.0 || coverage <= 0.0 {
        return vec4<f32>(0.0);
    }
    let point = in.local - in.shift;
    let origin = vec2<f32>(item.paint_origin.x, item.paint_origin.y);
    return paint_color(item.paint, point, origin) * min(coverage, 1.0) * clip;
}
