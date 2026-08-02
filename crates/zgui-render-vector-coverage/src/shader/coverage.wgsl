// Filling one outline by multisampling, into the accumulation texture of one pass.
//
// Every pixel of an item's box tests a fixed grid of sample points against that item's outline, and
// against every residual clip outline the pass could not bind, and keeps the fraction of samples
// that were inside all of them. Sixteen samples means an edge lands on one of seventeen levels; an
// interior is exact.
//
// This is the arrangement for a device with no compute shaders, so there is nothing here but a
// vertex stage, a fragment stage and two read-only storage buffers.

struct Item {
    // The quad, in the pass region's own pixels: origin then extent.
    bounds: vec4<f32>,
    // The extent of the scratch layer, which is what maps the quad into clip space. The layer's
    // and not the region's: a pass writes its region into the top-left of a larger layer.
    viewport: vec4<f32>,
    // Straight, gamma-encoded colour.
    color: vec4<f32>,
    // x: first segment. y: how many. z: non-zero for the even-odd rule. w: first clip run.
    control: vec4<f32>,
    // x: how many clip runs. yzw unused.
    clips: vec4<f32>,
}

@group(0) @binding(0) var<storage, read> items: array<Item>;
// Every outline in the frame, end to end: x0, y0, x1, y1.
@group(0) @binding(1) var<storage, read> segments: array<vec4<f32>>;
// Where each clip's outline starts, how long it is, and whether it is tested even-odd.
@group(0) @binding(2) var<storage, read> runs: array<vec4<f32>>;

// The sampling grid's side. Sixteen samples per pixel, which is the quality this trades for needing
// nothing but a fragment shader.
const GRID: i32 = 4;

struct Varying {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) instance: u32,
}

@vertex
fn vs_coverage(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32,
) -> Varying {
    let item = items[instance];
    let corner = vec2<f32>(f32(vertex & 1u), 0.5 * f32(vertex & 2u));
    let point = item.bounds.xy + corner * item.bounds.zw;
    let ndc = vec2<f32>(
        point.x / item.viewport.x * 2.0 - 1.0,
        1.0 - point.y / item.viewport.y * 2.0,
    );
    var out: Varying;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.instance = instance;
    return out;
}

@fragment
fn fs_coverage(in: Varying) -> @location(0) vec4<f32> {
    let item = items[in.instance];
    let first = u32(item.control.x);
    let count = u32(item.control.y);
    let even_odd = item.control.z != 0.0;
    let clip_first = u32(item.control.w);
    let clip_count = u32(item.clips.x);

    let corner = floor(in.position.xy);
    var inside = 0;
    for (var j = 0; j < GRID; j = j + 1) {
        for (var i = 0; i < GRID; i = i + 1) {
            let sample = corner + vec2<f32>(
                (f32(i) + 0.5) / f32(GRID),
                (f32(j) + 0.5) / f32(GRID),
            );
            if !contains(sample, first, count, even_odd) {
                continue;
            }
            // A residual clip is one the composite could not bind, so it is applied here — per
            // sample rather than as a separate coverage multiplied in afterwards, which is what
            // keeps the corner where an edge meets a clip from being lighter than either.
            var clipped = false;
            for (var c = 0u; c < clip_count; c = c + 1u) {
                let run = runs[clip_first + c];
                if !contains(sample, u32(run.x), u32(run.y), run.z != 0.0) {
                    clipped = true;
                    break;
                }
            }
            if !clipped {
                inside = inside + 1;
            }
        }
    }
    let coverage = f32(inside) / f32(GRID * GRID);
    if coverage <= 0.0 {
        return vec4<f32>(0.0);
    }
    // Premultiplied on the way into the accumulation texture, because that is the only form in
    // which source-over is a fixed-function blend. The resolve turns it back into the straight
    // colour the composite expects to read.
    let alpha = item.color.a * coverage;
    return vec4<f32>(item.color.rgb * alpha, alpha);
}

// Whether `point` is inside the outline held in `segments[first .. first + count]`.
fn contains(point: vec2<f32>, first: u32, count: u32, even_odd: bool) -> bool {
    var winding = 0;
    var crossings = 0;
    for (var index = 0u; index < count; index = index + 1u) {
        let segment = segments[first + index];
        let a = segment.xy;
        let b = segment.zw;
        if (a.y > point.y) == (b.y > point.y) {
            continue;
        }
        let at = a.x + (point.y - a.y) / (b.y - a.y) * (b.x - a.x);
        if at <= point.x {
            continue;
        }
        crossings = crossings + 1;
        if b.y > a.y {
            winding = winding + 1;
        } else {
            winding = winding - 1;
        }
    }
    if even_odd {
        return (crossings & 1) == 1;
    }
    return winding != 0;
}
