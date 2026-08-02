// Text decoration lines: underline, overline and strikethrough, in five styles.
//
// One primitive serves all three positions, because they differ only in where the rectangle sits.
// The styles differ in what fraction of the rectangle is inked, which is what this evaluates.

struct Decoration {
    order: u32,
    style: u32,
    bounds: Bounds,
    color: Rgba,
    thickness: f32,
    clip: u32,
    transform: u32,
    reserved: u32,
}

@group(1) @binding(0) var<storage, read> decorations: array<Decoration>;

const DECORATION_SOLID: u32 = 0u;
const DECORATION_WAVY: u32 = 1u;
const DECORATION_DASHED: u32 = 2u;
const DECORATION_DOTTED: u32 = 3u;
const DECORATION_DOUBLE: u32 = 4u;

struct DecorationVarying {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) instance: u32,
}

@vertex
fn vs_decoration(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32,
) -> DecorationVarying {
    let decoration = decorations[instance];
    let local = inflated_corner(vertex, decoration.bounds);
    var out: DecorationVarying;
    out.position = to_clip_position(local, decoration.transform);
    out.local = local;
    out.instance = instance;
    return out;
}

@fragment
fn fs_decoration(in: DecorationVarying) -> @location(0) vec4<f32> {
    let decoration = decorations[in.instance];
    let clip = clip_coverage(device_position(in.position.xy), decoration.clip);
    if clip <= 0.0 {
        return vec4<f32>(0.0);
    }

    let origin = bounds_origin(decoration.bounds);
    let size = bounds_size(decoration.bounds);
    let local = in.local - origin;
    let thickness = max(decoration.thickness, 1.0);
    let style = decoration.style;

    var coverage = 0.0;
    if style == DECORATION_WAVY {
        // The wave fills the rectangle's height, so its amplitude is what is left of the height
        // once the stroke has been drawn — which is what the ink rectangle was computed from.
        let amplitude = max((size.y - thickness) * 0.5, 0.0);
        let wavelength = max(thickness * 4.0, 1.0);
        let middle = size.y * 0.5;
        let wave = middle + amplitude * sin(local.x * 2.0 * M_PI / wavelength);
        coverage = saturate(0.5 + thickness * 0.5 - abs(local.y - wave));
    } else if style == DECORATION_DOUBLE {
        // Two lines, each a third of the height, with a third between them.
        let band = size.y / 3.0;
        let first = saturate(0.5 + band * 0.5 - abs(local.y - band * 0.5));
        let second = saturate(0.5 + band * 0.5 - abs(local.y - band * 2.5));
        coverage = max(first, second) * horizontal_extent(local, size);
    } else {
        coverage = horizontal_extent(local, size) * vertical_extent(local, size);
        if style == DECORATION_DASHED || style == DECORATION_DOTTED {
            // Dashes are proportional to the line's thickness for the same reason a border's are:
            // a dash shorter than the stroke is a smudge.
            let dash = select(thickness * 3.0, thickness, style == DECORATION_DOTTED);
            let period = dash * 2.0;
            let phase = sdf_fmod(local.x, period);
            coverage *= saturate(0.5 + dash - abs(phase - dash * 0.5) - dash * 0.5);
        }
    }

    return rgba_of(decoration.color) * coverage * clip;
}

// Coverage along the line, antialiased at both ends.
fn horizontal_extent(local: vec2<f32>, size: vec2<f32>) -> f32 {
    return saturate(0.5 + min(local.x, size.x - local.x));
}

// Coverage across the line, antialiased on both edges.
fn vertical_extent(local: vec2<f32>, size: vec2<f32>) -> f32 {
    return saturate(0.5 + min(local.y, size.y - local.y));
}
