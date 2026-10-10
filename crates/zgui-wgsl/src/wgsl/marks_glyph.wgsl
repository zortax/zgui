// Copies of repeated outlines of a mark payload, read from atlas cells.
//
// The payload starts with the item's tile table: sixteen words per outline, one per quarter-pixel
// phase, each `[x | y << 16, w | h << 16, ox, oy]`. The cell is in atlas texels and starts
// `(ox, oy)` from the pixel of the anchor. One instance per copy follows: its anchor as two `f32`,
// the outline it copies, and its own index from the first word, which is how it finds the table.
//
// The phase is taken from the anchor on the device, with the chunk offset in it, so a pan or a
// moved replay draws the cell of the new phase from the same payload. The cell is drawn on whole
// device pixels and read one texel at a time, with no contrast correction.

@group(2) @binding(1) var<storage, read> mark_glyphs: array<vec4<u32>>;
@group(3) @binding(0) var atlas: texture_2d<f32>;

struct GlyphVarying {
    @builtin(position) position: vec4<f32>,
    // The point in the item's own space, with the chunk offset added.
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) slot: u32,
    @location(2) @interpolate(flat) shift: vec2<f32>,
    // The device pixel the cell starts at.
    @location(3) @interpolate(flat) corner: vec2<f32>,
    // The cell in atlas texels: origin then extent.
    @location(4) @interpolate(flat) cell: vec4<i32>,
}

// The pixel a device coordinate is drawn from, and its quarter-pixel phase, per axis.
//
// Quantised first and split after, by the formula the cells were rasterised for.
fn glyph_phase(device: vec2<f32>) -> vec4<f32> {
    let quantised = floor(4.0 * device + vec2<f32>(0.5));
    let phase = quantised - 4.0 * floor(quantised / 4.0);
    return vec4<f32>((quantised - phase) / 4.0, phase);
}

// A device point in the clip space of the target, or of the bin page with `shift` added.
fn device_to_clip(device: vec2<f32>, shift: vec2<f32>) -> vec4<f32> {
    let texel = (device + shift) * globals.viewport.zw;
    return vec4<f32>(
        texel.x / globals.viewport.x * 2.0 - 1.0,
        1.0 - texel.y / globals.viewport.y * 2.0,
        0.0,
        1.0,
    );
}

fn glyph_corner(vertex: u32, instance: u32, coverage: bool) -> GlyphVarying {
    let found = item_of(mark_draw.position);
    let item = marks[found.slot];
    let record = mark_glyphs[instance];
    let base = instance - record.w;
    let anchor = mark_axes(item) * bitcast<vec2<f32>>(record.xy)
        + vec2<f32>(item.origin.x, item.origin.y) + found.shift;
    let matrix = spatial[item.transform].matrix;
    let world = matrix * vec4<f32>(anchor, 0.0, 1.0);
    let split = glyph_phase(world.xy / world.w);
    let phase = u32(split.z) + 4u * u32(split.w);
    let entry = mark_glyphs[base + 16u * record.z + phase];
    let size = vec2<u32>(entry.y & 0xffffu, entry.y >> 16u);
    let corner = split.xy + vec2<f32>(bitcast<vec2<i32>>(entry.zw));
    var device = corner + unit_corner(vertex) * vec2<f32>(size);
    if !coverage {
        // Out to whole texels of the target: a half-resolution texel holds two device pixels, and
        // a cell that ends on an odd one still has to reach the texel of its last pixel.
        let scale = globals.viewport.zw;
        let low = floor(corner * scale) / scale;
        let high = ceil((corner + vec2<f32>(size)) * scale) / scale;
        device = low + unit_corner(vertex) * (high - low);
    }
    // Back into the item's space, for the paint: the transform is affine on this route.
    let linear = mat2x2<f32>(matrix[0].xy, matrix[1].xy);
    let determinant = linear[0].x * linear[1].y - linear[1].x * linear[0].y;
    let inverse = mat2x2<f32>(
        vec2<f32>(linear[1].y, -linear[0].y),
        vec2<f32>(-linear[1].x, linear[0].x),
    ) * (1.0 / determinant);
    var out: GlyphVarying;
    if coverage {
        out.position = device_to_clip(device, mark_draw.shift);
    } else {
        out.position = device_to_clip(device, vec2<f32>(0.0));
    }
    out.local = inverse * (device - matrix[3].xy);
    out.slot = found.slot;
    out.shift = found.shift;
    out.corner = corner;
    out.cell = vec4<i32>(
        i32(entry.x & 0xffffu),
        i32(entry.x >> 16u),
        i32(size.x),
        i32(size.y),
    );
    return out;
}

// The coverage of the cell at the device pixel whose centre is `device`, and nothing outside it.
fn cell_coverage(in: GlyphVarying, device: vec2<f32>) -> f32 {
    let texel = vec2<i32>(floor(device - in.corner));
    if texel.x < 0 || texel.y < 0 || texel.x >= in.cell.z || texel.y >= in.cell.w {
        return 0.0;
    }
    return textureLoad(atlas, in.cell.xy + texel, 0).r;
}

@vertex
fn vs_glyph_paint(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32,
) -> GlyphVarying {
    return glyph_corner(vertex, instance, false);
}

@vertex
fn vs_glyph_coverage(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32,
) -> GlyphVarying {
    return glyph_corner(vertex, instance, true);
}

@fragment
fn fs_glyph_paint(in: GlyphVarying) -> @location(0) vec4<f32> {
    let item = marks[in.slot];
    let device = device_position(in.position.xy);
    var coverage = 0.0;
    if globals.viewport.z >= 1.0 {
        coverage = cell_coverage(in, floor(device) + vec2<f32>(0.5));
    } else {
        // The four device pixels whose centres this texel's footprint holds.
        let first = floor(device - vec2<f32>(0.5)) + vec2<f32>(0.5);
        coverage = 0.25 * (cell_coverage(in, first) + cell_coverage(in, first + vec2<f32>(1.0, 0.0))
            + cell_coverage(in, first + vec2<f32>(0.0, 1.0))
            + cell_coverage(in, first + vec2<f32>(1.0, 1.0)));
    }
    let clip = clip_coverage(device, item.clip);
    if clip <= 0.0 || coverage <= 0.0 {
        return vec4<f32>(0.0);
    }
    let point = in.local - in.shift;
    let origin = vec2<f32>(item.paint_origin.x, item.paint_origin.y);
    return paint_color(item.paint, point, origin) * coverage * clip;
}

@fragment
fn fs_glyph_coverage(in: GlyphVarying) -> @location(0) vec4<f32> {
    let coverage = cell_coverage(in, in.position.xy - mark_draw.shift);
    return select(vec4<f32>(0.0), vec4<f32>(coverage), in_bin(in.position.xy));
}
