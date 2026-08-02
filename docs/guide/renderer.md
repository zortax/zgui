# Writing a `Renderer`

A renderer takes a finished display list and a set of damage rectangles and puts pixels somewhere.
That is the whole of it. `zgui_render::Renderer` names no graphics API, so a second renderer — a
software one, one over a different API, a capture implementation that records instead of drawing —
is an ordinary implementation rather than a fork of anything.

This guide is what an implementor needs that the rustdoc on the trait does not say in one place:
what the display list already decided, what a renderer decides, and the three rules that are easy to
break and hard to notice.

## The contract

```rust,ignore
pub trait Renderer {
    fn capabilities(&self) -> RenderCapabilities;
    fn configure(&mut self, target: RenderTarget);
    fn target(&self) -> Option<RenderTarget>;
    fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome;
    fn register_external(&mut self, texture: ExternalTexture) -> TextureHandle;
    fn release_external(&mut self, handle: TextureHandle);
    fn memory(&self) -> MemoryReport;
    fn texture_sink(&mut self) -> &mut dyn TextureSink;
}
```

Eight methods, and the frame calls every one of them. There is no method here that exists for one
implementation's convenience.

`RenderTarget` is a description, not a resource: an extent in device pixels, the ratio between the
coordinates an author writes and the pixels the output has, and whether the surface is opaque. A
renderer owns whatever resources that description implies and reallocates them when it changes.

## Rule one: compose into a target you keep

`draw` is given the rectangles that must be redrawn and is entitled to assume that everything
outside them still holds the previous frame's pixels.

That is only legal because a renderer composes into a target it *keeps between frames* and then
copies or presents that target. An implementation that composes straight into whatever transient
surface the windowing system handed it this frame has to treat every frame as full damage, because
the surface it is given is not the surface it drew into last time.

If you are writing a renderer over a swap chain, this is the decision to make first, and it is not
reversible later without changing everything above it.

## Rule two: damage is retired on submission, not on presentation

`draw` returns a `FrameOutcome`, deliberately not a `Result`. Most of the ways a frame fails to
reach the screen are ordinary events in a window's life: the window was occluded, the surface went
stale under it, the compositor timed out.

What the caller actually has to know is whether the *work was submitted* — because a frame that
composed everything into its target and then failed to acquire a surface has still updated that
target. Redrawing it would repeat work that has already happened.

`FrameOutcome::retires_damage` is the authority, and it inverts the naive rule: every skip reason
retires damage **except** `SkipReason::Unconfigured`, which is the one case where nothing was
recorded at all.

Two skip reasons additionally say *do not ask for another frame*: `Occluded`, because a window that
is not visible will be made visible by a platform event and not by a busy loop; and `Undamaged`,
because a frame that damaged nothing would damage nothing next time either. Presenting an undamaged
frame also spends a swap-chain image copying the target onto the surface unchanged, which makes the
*next* frame wait a whole refresh interval for one.

## Rule three: publish capabilities before the frame is built, not after

`RenderCapabilities` is read *before* a display list is built, because these features change what
the list should contain and not merely how it is drawn:

- `subpixel_text` — per-channel text antialiasing needs dual-source blending, which is optional on
  real drivers and absent from software rasterisers. Where it is absent, ordinary coverage is
  emitted throughout. A pipeline gated on the feature with no fallback is a renderer that draws no
  text at all on those devices.
- `vector_compute` — whether the device can run a compute path rasteriser. Where it cannot, a
  simpler rasteriser is bound instead.
- `mutable_texture_formats` — decides whether a surface offering only an encoded format is handled
  by viewing it unencoded or by cancelling the encode in the final copy.
- `max_texture_size` — the atlas is sized against it.

`RenderCapabilities::MINIMAL` is the least capable device still worth supporting, and is what a
capture or software implementation should report unless it genuinely does better.

## What the display list already decided

A `Scene` handed to `draw` is **finished**: its arrays are in draw order and its vector passes are
planned. A renderer executes that plan. It does not derive one.

In particular:

- **Batching, ordering and overlap** are settled. Where one batch ends and the next begins is a
  property of the scene.
- **Where vector passes fall, how many there are, and what each one clips through** are decided in
  the display list before any renderer sees it.
- **The damage cull has already happened.** A renderer, and a `VectorRaster` under it, must not cull
  against damage a second time. Two owners of one decision is how the two come to disagree, and it
  would make a pass count into something only a real device could produce rather than something a
  test can assert about a scene.

The reason to be strict about this is testability: because every one of those decisions is in the
value, they can be asserted on with no graphics device present at all.

## The atlas seam

`texture_sink` is where rasterised content is uploaded. The split is worth understanding: the
atlas's *policy* — which tile goes where, what is held, what is evicted — is decided above a
renderer with no device in sight, in `zgui-atlas`. What a renderer owns is the textures themselves,
because they have to be created on the device that will sample them.

The sink is handed out mutably and per call rather than held by whoever caches, because the textures
do not survive `configure` on a lost device. A borrow kept across frames would outlive what it
names.

## Vector content: `VectorRaster`

Path rendering is a second, separate contract, so that a device without compute can fall back to a
simpler rasteriser without the renderer changing.

```rust,ignore
pub trait VectorRaster: 'static {
    fn plan(&mut self, passes: &ScenePassPlan) -> VectorPlan;
    fn clear_targets(&mut self, plan: &VectorPlan);
    fn prepare(&mut self, frame: &VectorFrame<'_>) -> Result<(), VectorError>;
    fn memory(&self) -> MemoryReport;
}
```

Four things an implementor must get right:

**Straight colour out.** An implementation produces un-premultiplied colour; the compositing draw
premultiplies. Content outside a requested region is left untouched.

**The plan is index-aligned with what it was given** — one `VectorPass` per `PlannedPass`, in the
same order, or an empty plan. The display list names each composite by the *plan's* index, so an
implementation that quietly dropped a pass it did not like would not draw one composite fewer; it
would draw every later composite from the wrong pass. `VectorPlan::resourcing` starts a plan that
keeps the alignment.

**`clear_targets` is mandatory, not an optimisation.** An implementation whose rasterisation can
fail while reporting success would otherwise leave a reused scratch buffer holding the *previous*
frame's content. That composites as wrong pixels rather than missing ones, and there is nothing to
notice it by.

**Residual clips are part of the contract.** One composite applies one clip, so an item whose clip
chain runs deeper than its pass's has the extra links applied *inside* the scratch. There is no
fallback to specify, because anything an implementation could not express was turned into a pass
boundary before the plan was made.

An empty plan means the caller must do nothing at all: an empty pass over a full-size surface is not
free.

## Testing an implementation

You do not need a window. `zgui-testkit-scene` builds scenes with no graphics device and no font
files, and gives a golden-dump format for comparing them. The workspace ships a capture renderer
used exactly this way, and the useful shape of a test is:

1. build a scene and a damage set from a fixture,
2. draw it with your renderer and with the capture renderer,
3. compare the primitives each one was asked to draw, then compare pixels only where pixels are the
   question.

A renderer that only ever runs against real hardware in a real window has no cheap way to answer
"did this frame draw the right things?", which is the question that fails most often.

## Installing it

An application replaces exactly one decision:

```rust,ignore
zgui::app()
    .with_renderer(my_renderer_factory)
    .run(|| view! { App() })
```

Nothing above the renderer changes, because nothing above it names a graphics API.
