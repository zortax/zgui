# Drawing through a compositor

zgui draws nothing itself. Each frame, the paint stage records what changed as compositor items and
commits them to a compositor in one transaction. The compositor builds frames from what it holds,
and a renderer draws those frames into a window, a buffer or a transcript. The compositor and the
renderer are trait objects, `Box<dyn compositor_api::Compositor>` and
`Box<dyn compositor_api::Renderer>`, and zgui relies only on the behaviour the shared compositor
specification guarantees. Any implementation that meets the specification can draw a zgui window.

This guide says what zgui sends to a compositor, where the implementations are chosen, and how an
application or a test chooses others. The specification is the contract for an implementor.

## What a window sends

Each window is one client of the application's compositor and draws into one output of it.

- **Chunks.** A fragment's painting is one chunk of items: quads, shadows, glyph runs, lines,
  paths, marks, images, shader quads and external textures. The chunk key names the box and the
  fragment's position among the box's fragments, so a fragment rebuilt in its box's place keeps its
  key. A fragment that changed is recorded again. A fragment that only moved keeps its chunk, and
  the window writes the chunk's offset. A retired fragment's chunk is removed.
- **Property trees.** Coordinate systems become transform nodes, scroll containers become scroll
  nodes, clip chains become clip nodes, and paints, images, shaders and parameter blocks become
  slots. A change to one of zgui's tables is written to the node or slot it is mirrored as, so an
  animated colour or a scroll offset is drawn without recording a chunk again.
- **Order.** The painting order is an order tree, with an effect node where a group needs a target
  of its own for opacity, a filter or a blend mode.

zgui's own damage decides which fragments the paint walk visits. The compositor works out from the
recorded changes which pixels each frame redraws, and whether a scroll is drawn as a shift of the
previous picture.

The engine crates name only `compositor-types` and `compositor-api`. `cargo xtask ledger
compositor` fails any other crate that names an implementation or downcasts to one.

## Where the implementations are chosen

`zgui::app()` chooses them, in the module `zgui::app::compositor`:

- The compositor is the reference implementation, `compositor-core`, with glyphs rasterised by
  swash and its atlas held to a budget.
- Each window gets a `compositor-wgpu` renderer that presents into its surface. All windows share
  one device and one pipeline cache, kept in `ZGUI_CACHE_DIR` or in `~/.cache/zgui/pipelines`.
- Paths are rasterised by vello on the same device.

`zgui::compositor` re-exports the interface, `compositor-api` and `compositor-types`, for an
application that names compositor types.

A few environment variables tune the default renderer:

- `ZGUI_GROUP_PRECISION` sets the precision of group targets.
- `ZGUI_PRESENT_PACING=notify` tells a display-paced surface that a frame is about to be
  presented.
- `ZGUI_TEXT_AA` selects the text antialiasing: `auto`, `gray`, `rgb` or `bgr`.

## Choosing another renderer

An application replaces one decision:

```rust,ignore
zgui::app()
    .with_renderer(Box::new(|surface, target| my_renderer(surface, target)))
    .run(|| view! { App() })
```

The factory is a `zgui_runtime::RendererFactory`. It is called once per window, with the window's
surface and its first render target, and returns the window's renderer or an `AppError`.
`App::with_compositor_renderer` installs a renderer that draws off screen instead, built for the
window's first size. The runtime's `App::with_compositor` installs the compositor.

A renderer follows the renderer half of the specification. Three of its rules decide whether
zgui's frames come out right:

- A renderer applies a frame's resources, chunk notes and table deltas even when it skips the
  frame, so the next frame's deltas stay valid.
- An outcome that retires a frame's damage means the frame was drawn. An outcome that keeps it
  makes the compositor redraw that damage in a later frame.
- The capabilities a renderer reports are read before frames are built. A change of them redraws
  the whole output.

## Testing

Every zgui test that needs a compositor or a renderer takes it from `zgui-testkit-compositor`, so a
test runs against whichever implementation the environment selects:

- `ZGUI_COMPOSITOR` selects the compositor: `core`, the default, or `naive`.
- `ZGUI_RENDERER` selects the renderer: `capture`, the default, `wgpu`, `cpu` or `null`.

The `capture` renderer records each frame as a transcript, so a test reads what a frame draws with
no graphics device. An implementation that does not exist yet, an unknown value and a device that
cannot be opened are reported as unavailable, and the test prints the reason and returns. The
factory never substitutes another implementation.

`zgui-testkit` holds the instruments that need no compositor: a shaper with fixed metrics and no
font files, the frame counter assertions, and the golden dumps.
