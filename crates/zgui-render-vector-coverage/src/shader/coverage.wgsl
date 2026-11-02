// Filling one outline by multisampling, into the accumulation texture of one pass.
//
// Every pixel of an item's box tests a fixed grid of sample points against that item's outline, and
// against every residual clip outline the pass could not bind, and keeps the fraction of samples
// that were inside all of them. Sixteen samples means an edge lands on one of seventeen levels; an
// interior is exact.
//
// This is the arrangement for a device with no compute shaders, so there is nothing here but a
// vertex stage, a fragment stage and read-only textures.

struct Item {
    // The quad, in the pass region's own pixels: origin then extent.
    bounds: vec4<f32>,
    // The extent of the scratch layer, which is what maps the quad into clip space. The layer's
    // and not the region's: a pass writes its region into the top-left of a larger layer.
    viewport: vec4<f32>,
    // Straight, gamma-encoded colour.
    color: vec4<f32>,
    // x: first band. y: how many. z: non-zero for the even-odd rule. w: first clip run.
    control: vec4<f32>,
    // x: how many clip runs. y: where the first band begins. z: how tall one band is. w unused.
    bands: vec4<f32>,
}

// Textures rather than storage buffers, so that a device with no storage buffers at all — a GL 3.3
// context, WebGL 2 — can run this. It is the rasteriser such a device falls back *to*, so it above
// all must not ask for what that device has none of.
@group(0) @binding(0) var items: texture_2d<u32>;
// Every outline the fragment stage walks: each band's own segments, then each clip run's. One
// texel per segment — x0, y0, x1, y1.
@group(0) @binding(1) var outlines: texture_2d<u32>;
// Where each clip's outline starts, how long it is, and whether it is tested even-odd.
@group(0) @binding(2) var runs: texture_2d<u32>;
// Per band: where its own segments start, and how many there are.
@group(0) @binding(3) var bands: texture_2d<u32>;

/// How many texels wide every one of them is. `TableTexture` uses the same number.
const TABLE_TEXELS_WIDE: u32 = 256u;

/// Where texel `index` of a table is.
fn table_texel(index: u32) -> vec2<i32> {
    return vec2<i32>(
        i32(index % TABLE_TEXELS_WIDE),
        i32(index / TABLE_TEXELS_WIDE),
    );
}

/// One segment, which is one texel.
fn load_segment(index: u32) -> vec4<f32> {
    return bitcast<vec4<f32>>(textureLoad(outlines, table_texel(index), 0));
}

/// One clip run, which is one texel.
fn load_run(index: u32) -> vec4<f32> {
    return bitcast<vec4<f32>>(textureLoad(runs, table_texel(index), 0));
}

/// One item, which spans 5 texels of the arena.
fn load_item(slot: u32) -> Item {
    let base = slot * 5u;
    let t0 = textureLoad(items, table_texel(base + 0u), 0);
    let t1 = textureLoad(items, table_texel(base + 1u), 0);
    let t2 = textureLoad(items, table_texel(base + 2u), 0);
    let t3 = textureLoad(items, table_texel(base + 3u), 0);
    let t4 = textureLoad(items, table_texel(base + 4u), 0);
    return Item(
        vec4<f32>(bitcast<f32>(t0.x), bitcast<f32>(t0.y), bitcast<f32>(t0.z), bitcast<f32>(t0.w)),
        vec4<f32>(bitcast<f32>(t1.x), bitcast<f32>(t1.y), bitcast<f32>(t1.z), bitcast<f32>(t1.w)),
        vec4<f32>(bitcast<f32>(t2.x), bitcast<f32>(t2.y), bitcast<f32>(t2.z), bitcast<f32>(t2.w)),
        vec4<f32>(bitcast<f32>(t3.x), bitcast<f32>(t3.y), bitcast<f32>(t3.z), bitcast<f32>(t3.w)),
        vec4<f32>(bitcast<f32>(t4.x), bitcast<f32>(t4.y), bitcast<f32>(t4.z), bitcast<f32>(t4.w)),
    );
}

// Where the sample grid's four rows and four columns sit inside a pixel. Sixteen samples per pixel,
// which is the quality this trades for needing nothing but a fragment shader.
const OFFSETS: vec4<f32> = vec4<f32>(0.125, 0.375, 0.625, 0.875);

/// What the sixteen samples of one pixel have counted, a row of four to a vector.
///
/// Four vectors rather than an array, because an array a loop counter indexes becomes addressable
/// storage on a driver that cannot prove the index away, and that storage is off the chip.
struct Rows {
    first: vec4<i32>,
    second: vec4<i32>,
    third: vec4<i32>,
    fourth: vec4<i32>,
}

/// Nothing counted yet.
fn no_rows() -> Rows {
    return Rows(vec4<i32>(0), vec4<i32>(0), vec4<i32>(0), vec4<i32>(0));
}

/// What one segment's crossing of the row at `y` adds to that row's four samples.
///
/// `slope` is the segment's own dx/dy, which is one division for the four rows rather than one for
/// each. The ray is cast to the right, so a crossing counts for every sample it is to the right of
/// — and those are a prefix of the row, which makes the whole row one comparison against one number.
fn crossing(a: vec2<f32>, ends: f32, slope: f32, y: f32, columns: vec4<f32>, delta: i32) -> vec4<i32> {
    if (a.y > y) == (ends > y) {
        return vec4<i32>(0);
    }
    let at = a.x + (y - a.y) * slope;
    // Built from the comparison rather than selected on it, because `select` over an integer
    // vector reaches GLSL 330 as a `mix` that version defines for floating point only.
    return vec4<i32>(columns < vec4<f32>(at)) * delta;
}

/// What the segments `first .. first + count` do to each of a pixel's sixteen samples.
///
/// One pass over the outline serves the whole pixel: a segment is fetched once and then asked where
/// it crosses each of the four sample rows. Fetching it once for each row instead is four times the
/// memory traffic for the same answer.
///
/// Even-odd counts crossings and non-zero sums their directions, so one accumulator serves both
/// rules — a step of one for the first, the crossing's own direction for the second.
fn wind(first: u32, count: u32, rows: vec4<f32>, columns: vec4<f32>, even_odd: bool) -> Rows {
    var out = no_rows();
    for (var index = 0u; index < count; index = index + 1u) {
        let segment = load_segment(first + index);
        let a = segment.xy;
        let b = segment.zw;
        let delta = select(select(-1, 1, b.y > a.y), 1, even_odd);
        // The one division the segment needs, taken out of the four rows that share it. A segment
        // is never horizontal — one of those crosses no row and is dropped where it is flattened —
        // so this never divides by nothing.
        let slope = (b.x - a.x) / (b.y - a.y);
        out.first = out.first + crossing(a, b.y, slope, rows.x, columns, delta);
        out.second = out.second + crossing(a, b.y, slope, rows.y, columns, delta);
        out.third = out.third + crossing(a, b.y, slope, rows.z, columns, delta);
        out.fourth = out.fourth + crossing(a, b.y, slope, rows.w, columns, delta);
    }
    return out;
}

/// The fill rule applied to one row of four counts: one where the sample is inside, zero where not.
fn holds(counted: vec4<i32>, even_odd: bool) -> vec4<i32> {
    if even_odd {
        return counted & vec4<i32>(1);
    }
    return vec4<i32>(counted != vec4<i32>(0));
}

/// Which of a pixel's sixteen samples the counted crossings put inside the outline.
fn filled(counted: Rows, even_odd: bool) -> Rows {
    return Rows(
        holds(counted.first, even_odd),
        holds(counted.second, even_odd),
        holds(counted.third, even_odd),
        holds(counted.fourth, even_odd),
    );
}

/// The samples both masks hold.
fn both(left: Rows, right: Rows) -> Rows {
    return Rows(
        left.first & right.first,
        left.second & right.second,
        left.third & right.third,
        left.fourth & right.fourth,
    );
}

/// How many of the sixteen samples a mask holds.
fn tally(mask: Rows) -> i32 {
    let sum = mask.first + mask.second + mask.third + mask.fourth;
    return sum.x + sum.y + sum.z + sum.w;
}

struct Varying {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) color: vec4<f32>,
    @location(1) @interpolate(flat) control: vec4<f32>,
    @location(2) @interpolate(flat) bands: vec4<f32>,
}

@vertex
fn vs_coverage(
    @builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32,
) -> Varying {
    let item = load_item(instance);
    let corner = vec2<f32>(f32(vertex & 1u), 0.5 * f32(vertex & 2u));
    let point = item.bounds.xy + corner * item.bounds.zw;
    let ndc = vec2<f32>(
        point.x / item.viewport.x * 2.0 - 1.0,
        1.0 - point.y / item.viewport.y * 2.0,
    );
    var out: Varying;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    // Carried across rather than fetched again per fragment. A record is five texels and a fragment
    // reads three of them, so forwarding costs interpolator slots the device has to spare and saves
    // three fetches on every pixel of every item.
    out.color = item.color;
    out.control = item.control;
    out.bands = item.bands;
    return out;
}

@fragment
fn fs_coverage(in: Varying) -> @location(0) vec4<f32> {
    let band_first = u32(in.control.x);
    let band_count = u32(in.control.y);
    let even_odd = in.control.z != 0.0;
    let clip_first = u32(in.control.w);
    let clip_count = u32(in.bands.x);
    let top = in.bands.y;
    let tall = in.bands.z;
    if band_count == 0u {
        return vec4<f32>(0.0);
    }

    let corner = floor(in.position.xy);
    let columns = corner.x + OFFSETS;
    let rows = corner.y + OFFSETS;

    // A band is a whole number of pixels tall and begins on a pixel boundary, so the one band this
    // pixel sits in holds every segment that can cross any of its sixteen samples. That is what
    // lets the band be found once and walked once, rather than once for each row of four.
    let which = clamp(i32((corner.y - top) / tall), 0, i32(band_count) - 1);
    let band = textureLoad(bands, table_texel(band_first + u32(which)), 0);
    var inside = filled(wind(band.x, band.y, rows, columns, even_odd), even_odd);

    // A residual clip is one the composite could not bind, so it is applied here — per sample
    // rather than as a separate coverage multiplied in afterwards, which is what keeps the corner
    // where an edge meets a clip from being lighter than either.
    for (var c = 0u; c < clip_count; c = c + 1u) {
        let run = load_run(clip_first + c);
        let clip_even_odd = run.z != 0.0;
        let counted = wind(u32(run.x), u32(run.y), rows, columns, clip_even_odd);
        inside = both(inside, filled(counted, clip_even_odd));
    }

    let coverage = f32(tally(inside)) / 16.0;
    if coverage <= 0.0 {
        return vec4<f32>(0.0);
    }
    // Premultiplied on the way into the accumulation texture, because that is the only form in
    // which source-over is a fixed-function blend. The resolve turns it back into the straight
    // colour the composite expects to read.
    let alpha = in.color.a * coverage;
    return vec4<f32>(in.color.rgb * alpha, alpha);
}
