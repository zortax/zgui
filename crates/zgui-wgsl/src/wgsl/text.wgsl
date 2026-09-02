// DERIVED-FROM: the Windows Terminal project, src/renderer/atlas/dwrite.hlsl (MIT)
// Copyright (c) Microsoft Corporation.
// The contrast enhancement and the cubic alpha correction are adapted from that work, which is
// distributed under the MIT License, and have been modified: the coefficients arrive as a uniform
// computed once from the display gamma rather than being recomputed here, and the single-channel
// and per-channel forms share one set of helpers.

// Perceived brightness, with the luminance coefficients of REC. 601.
fn color_brightness(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.30, 0.59, 0.11));
}

// Light text on a dark background needs more contrast enhancement than the other way round,
// because thin strokes of a bright colour bleed into their background.
fn light_on_dark_contrast(enhanced_contrast: f32, color: vec3<f32>) -> f32 {
    let multiplier = saturate(4.0 * (0.75 - color_brightness(color)));
    return enhanced_contrast * multiplier;
}

fn enhance_contrast(alpha: f32, k: f32) -> f32 {
    return alpha * (k + 1.0) / (alpha * k + 1.0);
}

fn enhance_contrast3(alpha: vec3<f32>, k: f32) -> vec3<f32> {
    return alpha * (k + 1.0) / (alpha * k + 1.0);
}

fn apply_alpha_correction(a: f32, b: f32, g: vec4<f32>) -> f32 {
    let brightness_adjustment = g.x * b + g.y;
    let correction = brightness_adjustment * a + (g.z * b + g.w);
    return a + a * (1.0 - a) * correction;
}

fn apply_alpha_correction3(a: vec3<f32>, b: vec3<f32>, g: vec4<f32>) -> vec3<f32> {
    let brightness_adjustment = g.x * b + g.y;
    let correction = brightness_adjustment * a + (g.z * b + g.w);
    return a + a * (1.0 - a) * correction;
}

// Corrects one coverage value for the display's contrast and gamma.
fn correct_coverage(sample: f32, color: vec3<f32>, enhanced_contrast_factor: f32) -> f32 {
    let contrast = light_on_dark_contrast(enhanced_contrast_factor, color);
    let contrasted = enhance_contrast(sample, contrast);
    return apply_alpha_correction(contrasted, color_brightness(color), globals.gamma_ratios);
}

// Corrects three per-channel coverage values the same way.
fn correct_coverage3(sample: vec3<f32>, color: vec3<f32>, enhanced_contrast_factor: f32) -> vec3<f32> {
    let contrast = light_on_dark_contrast(enhanced_contrast_factor, color);
    let contrasted = enhance_contrast3(sample, contrast);
    return apply_alpha_correction3(contrasted, color, globals.gamma_ratios);
}
