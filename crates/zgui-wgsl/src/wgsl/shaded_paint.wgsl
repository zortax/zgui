// A paint effect: the application shades the colour, and the framework applies the box.
//
// The rounded rectangle still bounds it. An effect that wants to draw past its own corners gives
// itself square corners and shapes what it returns.

@fragment
fn fs_shaded_paint(in: ShadedVarying) -> @location(0) vec4<f32> {
    let quad = shaded[in.instance];
    // The clip is in device space, so it is evaluated at the real pixel; the box is in the
    // primitive's own space, so it is evaluated at the point that maps to this pixel.
    let clip = clip_coverage(device_position(in.position.xy), quad.clip);
    if clip <= 0.0 {
        return vec4<f32>(0.0);
    }
    let point = in.local - in.shift;
    let input = shader_input(quad, point);
    // Outside the unit range a premultiplied colour has no meaning, and one frame of a wrong
    // shader would otherwise reach the screen as saturated noise.
    let color = clamp(shade(input, shader_params.user), vec4<f32>(0.0), vec4<f32>(1.0));
    let shape = coverage_of(quad_sdf(point, quad.bounds, quad.radii, CORNER_ROUND));
    return color * shape * clip * quad.opacity;
}
