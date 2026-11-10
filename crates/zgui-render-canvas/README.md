# zgui-render-canvas

Canvas 2D backend for browser WebAssembly applications that cannot obtain a
WebGPU adapter. The public API is available on `wasm32`.

```rust,ignore
let renderer = zgui_render_canvas::CanvasRenderer::new(canvas)?;
```

Return the renderer from `App::with_renderer`, then call the browser startup API.
The selected HTML canvas must have no existing WebGPU or WebGL context. Browser
canvases fix their rendering context type on first use.

## Rendering pipeline

zgui performs CSS styling, layout, text shaping, glyph rasterization, vector
lowering, hit testing and reactive updates. This backend consumes the resulting
`Scene` batches and the existing atlas `TextureSink` uploads. Text reaches Canvas
as glyph image data; it does not use browser text layout or `fillText`.

The canvas retains composed pixels. Each draw clears and redraws the supplied
damage regions. The finished scene's vector pass plan controls which paths draw
and where they enter the painting order. Rounded clip chains and scene transforms
apply to primitives and atlas glyphs. Groups use separate canvases so group
opacity composites overlapping children once.

The CPU atlas preserves zgui's R8 coverage and premultiplied RGBA formats.
Converted tile canvases use straight RGBA `ImageData`. Converted tiles are
invalidated on atlas writes and can be released through the renderer cache API.

## Supported subset

- Solid rounded rectangles, elliptical corner radii and solid borders, including
  independently colored sides.
- Monochrome atlas glyphs, color glyphs and decoded image sprites, with opacity,
  framing and rounded image clips.
- General Bézier fills, nonzero and even-odd winding, path clips and strokes
  expanded by kurbo with their original caps, joins and dash patterns.
- Analytic discs, rings, squares, square rings, rounded boxes and polylines in
  mark payloads.
- Non-repeating linear and radial gradients, interpolated in browser sRGB.
- Affine 2D transforms, arbitrary-depth rounded rectangle clip chains and group
  opacity. Browser Canvas blend modes and standard CSS filter functions are used
  for groups. Outset box shadows use the browser blur implementation.
- Reactive paint changes use the scene's current paint values and glyph colors.

## Limits

This is an initial compatibility backend. It advertises grayscale text and no
custom shader support. Canvas antialiasing and blur can differ from the WebGPU
renderer. Glyph coverage uses the rasterizer's mask directly; the WebGPU text
shader's additional contrast correction is not applied.

External GPU textures, custom shaders, backdrop filters, inset box shadows,
sampled image paint fills and instanced vector glyph sheets are skipped with a
console warning. Ordinary glyph and image sprites remain supported. Raster mask
clips hide their affected content and emit a warning.

Superellipse corners use elliptical curves. Dashed and dotted box borders and
non-solid text decorations use solid strokes. Repeating and conic gradients use
their mean color. Gradient interpolation in other color spaces uses browser
sRGB. Screen-sized marks use local-space sizes, and 3D transforms use their 2D
affine components. Each approximation emits a warning once per renderer.

The backend currently allocates a full viewport canvas for each active isolated
group. It has no scroll-copy optimization or GPU resource sharing. Glyph tile
conversion and atlas invalidation favor simple correctness over throughput.
