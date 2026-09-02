// A coverage effect: the application shapes the box, and the framework fills it.
//
// The fill and the border are the ordinary paint references, so a gradient, an image and a border
// colour all keep working on a shape the framework has never heard of.
//
// The border is the ring between the shape and the same shape evaluated on the box shrunk by the
// border widths. For a superellipse that inset shape is the offset curve; for an arbitrary
// coverage function it is an approximation of one.

@fragment
fn fs_shaded_coverage(in: ShadedVarying) -> @location(0) vec4<f32> {
    let quad = shaded[in.instance];
    let clip = clip_coverage(device_position(in.position.xy), quad.clip);
    if clip <= 0.0 {
        return vec4<f32>(0.0);
    }
    let point = in.local - in.shift;
    let paint_origin = vec2<f32>(quad.paint_origin.x, quad.paint_origin.y);
    let input = shader_input(quad, point);
    let outer = saturate(coverage(input, shader_params.user));
    if outer <= 0.0 {
        return vec4<f32>(0.0);
    }

    let inset = vec2<f32>(
        0.5 * (quad.border.left + quad.border.right),
        0.5 * (quad.border.top + quad.border.bottom),
    );
    var inner = outer;
    if inset.x > 0.0 || inset.y > 0.0 {
        var shrunk = input;
        shrunk.size = max(input.size - 2.0 * inset, vec2<f32>(1.0e-4));
        shrunk.local = input.local - inset;
        shrunk.uv = shrunk.local / shrunk.size;
        inner = saturate(coverage(shrunk, shader_params.user));
    }

    // The two are disjoint — the inner shape is contained in the outer one — so the ring between
    // them is a sum rather than a composite.
    let background = paint_color(quad.fill, point, paint_origin) * inner;
    let border = paint_color(quad.stroke, point, paint_origin) * max(outer - inner, 0.0);
    return (background + border) * clip * quad.opacity;
}
