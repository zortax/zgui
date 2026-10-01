// Converts one video frame's Y'CbCr planes to gamma-encoded BT.709 R'G'B' in a full-target
// triangle.
//
// Native frames (SDR on BT.709 primaries) stop after the matrix. Mapped frames decode to linear
// light relative to reference white, tone map HDR highlights with the BT.2390 EETF on the
// largest channel, convert their primaries to BT.709, and encode with the sRGB curve.

struct Params {
    // One output channel per row: weights for the luma, Cb and Cr planes, then an offset.
    red: vec4<f32>,
    green: vec4<f32>,
    blue: vec4<f32>,
    // xy: the visible fraction of the luma plane. z: 1 when Cr is the chroma plane's second
    // channel. w: the factor that turns a sampled value into a normalised code.
    shape: vec4<f32>,
    // xy: the chroma siting shift, in normalised chroma coordinates.
    chroma: vec4<f32>,
    // Linear source primaries to linear BT.709, one row each.
    gamut_red: vec4<f32>,
    gamut_green: vec4<f32>,
    gamut_blue: vec4<f32>,
    // x: the decoding. y: the exponent of a power decoding. z: the HLG system gamma. w: the HLG
    // display peak relative to reference white.
    transfer: vec4<f32>,
    // x: 1 when tone mapping. y: the PQ signal of the source peak. z: the knee. w: the target
    // peak as a fraction of the source peak's PQ signal.
    tone: vec4<f32>,
    // xyz: the luminance weights of the source primaries.
    luminance: vec4<f32>,
}

// Whether the frame passes through linear light.
override MAPPED: bool = false;

const TRANSFER_SRGB: u32 = 0u;
const TRANSFER_LINEAR: u32 = 1u;
const TRANSFER_PQ: u32 = 2u;
const TRANSFER_HLG: u32 = 3u;
const TRANSFER_POWER: u32 = 4u;

// Diffuse white in HDR content, in nits (ITU-R BT.2408).
const REFERENCE_WHITE: f32 = 203.0;

const PQ_M1: f32 = 0.1593017578125;
const PQ_M2: f32 = 78.84375;
const PQ_C1: f32 = 0.8359375;
const PQ_C2: f32 = 18.8515625;
const PQ_C3: f32 = 18.6875;

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var bilinear: sampler;
@group(0) @binding(2) var luma: texture_2d<f32>;
@group(0) @binding(3) var cb: texture_2d<f32>;
@group(0) @binding(4) var cr: texture_2d<f32>;

struct Varyings {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> Varyings {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: Varyings;
    out.position = vec4<f32>(corner * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = corner * params.shape.xy;
    return out;
}

fn srgb_decode(v: vec3<f32>) -> vec3<f32> {
    return select(pow((v + 0.055) / 1.055, vec3<f32>(2.4)), v / 12.92, v <= vec3<f32>(0.04045));
}

fn srgb_encode(v: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(v, vec3<f32>(1.0 / 2.4)) - 0.055, v * 12.92, v <= vec3<f32>(0.0031308));
}

// SMPTE ST 2084: a PQ signal to luminance as a fraction of 10 000 nits.
fn pq_decode(e: vec3<f32>) -> vec3<f32> {
    let p = pow(e, vec3<f32>(1.0 / PQ_M2));
    return pow(max(p - PQ_C1, vec3<f32>(0.0)) / (PQ_C2 - PQ_C3 * p), vec3<f32>(1.0 / PQ_M1));
}

fn pq_decode1(e: f32) -> f32 {
    return pq_decode(vec3<f32>(e)).x;
}

fn pq_encode1(y: f32) -> f32 {
    let p = pow(clamp(y, 0.0, 1.0), PQ_M1);
    return pow((PQ_C1 + PQ_C2 * p) / (1.0 + PQ_C3 * p), PQ_M2);
}

// ARIB STD-B67: an HLG signal to relative scene light.
fn hlg_decode(e: vec3<f32>) -> vec3<f32> {
    let a = 0.17883277;
    let b = 0.28466892;
    let c = 0.55991073;
    return select((exp((e - c) / a) + b) / 12.0, e * e / 3.0, e <= vec3<f32>(0.5));
}

// Decoded R'G'B' to linear light, with 1.0 at reference white.
fn to_linear(encoded: vec3<f32>) -> vec3<f32> {
    let v = max(encoded, vec3<f32>(0.0));
    switch u32(params.transfer.x) {
        case TRANSFER_LINEAR: {
            return v;
        }
        case TRANSFER_PQ: {
            return pq_decode(v) * (10000.0 / REFERENCE_WHITE);
        }
        case TRANSFER_HLG: {
            // The BT.2100 OOTF for a display of the stated peak.
            let scene = hlg_decode(v);
            let y = max(dot(params.luminance.xyz, scene), 1e-6);
            return scene * pow(y, params.transfer.z - 1.0) * params.transfer.w;
        }
        case TRANSFER_POWER: {
            return pow(v, vec3<f32>(params.transfer.y));
        }
        default: {
            return srgb_decode(v);
        }
    }
}

// ITU-R BT.2390 EETF on the largest channel: highlights roll off into reference white.
fn tone_map(light: vec3<f32>) -> vec3<f32> {
    let largest = max(light.r, max(light.g, light.b));
    if largest <= 0.0 {
        return light;
    }
    let source = params.tone.y;
    let knee = params.tone.z;
    let target_peak = params.tone.w;
    let e1 = min(pq_encode1(largest * REFERENCE_WHITE / 10000.0) / source, 1.0);
    var e2 = e1;
    if e1 > knee {
        let t = (e1 - knee) / (1.0 - knee);
        let t2 = t * t;
        let t3 = t2 * t;
        e2 = (2.0 * t3 - 3.0 * t2 + 1.0) * knee + (t3 - 2.0 * t2 + t) * (1.0 - knee)
            + (-2.0 * t3 + 3.0 * t2) * target_peak;
    }
    let mapped = pq_decode1(e2 * source) * (10000.0 / REFERENCE_WHITE);
    return light * (mapped / largest);
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    let chroma_uv = in.uv + params.chroma.xy;
    let y = textureSampleLevel(luma, bilinear, in.uv, 0.0).r;
    let blue_difference = textureSampleLevel(cb, bilinear, chroma_uv, 0.0);
    let red_difference = textureSampleLevel(cr, bilinear, chroma_uv, 0.0);
    let codes = vec3<f32>(
        y,
        blue_difference.r,
        select(red_difference.r, red_difference.g, params.shape.z > 0.5),
    ) * params.shape.w;
    let sample = vec4<f32>(codes, 1.0);
    let encoded = vec3<f32>(dot(params.red, sample), dot(params.green, sample), dot(params.blue, sample));
    if !MAPPED {
        return vec4<f32>(clamp(encoded, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
    }

    var light = to_linear(encoded);
    if params.tone.x > 0.5 {
        light = tone_map(light);
    }
    let bt709 = vec3<f32>(
        dot(params.gamut_red.xyz, light),
        dot(params.gamut_green.xyz, light),
        dot(params.gamut_blue.xyz, light),
    );
    return vec4<f32>(srgb_encode(clamp(bt709, vec3<f32>(0.0), vec3<f32>(1.0))), 1.0);
}
