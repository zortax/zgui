# Architecture

This document describes what zgui is made of and how one frame gets from a person moving a pointer
to pixels on a screen. It is the map the other guides assume: [layering](layering.md) states the
rules the map obeys, [the styling model](styling.md), [the reactive model](reactivity.md) and
[writing a renderer](renderer.md) each take one region of it in detail, and [building a browser on
zgui](browser.md) names the places a consumer plugs into.

## The shape of it

zgui is a **retained** framework. It keeps the document, computed styles, box tree, shaped
paragraphs, per-fragment paint recordings, and rendered pixels. A frame rebuilds only the data that
became invalid. The paint stage assembles a finished scene from recordings that intersect the damage
set, and the renderer keeps pixels outside that set.

```text
  components and signals            zgui-view, zgui-view-macro, zgui-elements, zgui-reactive
            │
            │  create / update nodes through the `Dom` seam
            ▼
  the document                      zgui-dom
            │
            │  invalidation, as obligations on nodes
            ▼
  the cascade                       zgui-style over zgui-css
            │
            │  computed styles, plus the damage they imply
            ▼
  the box tree and layout           zgui-layout, zgui-text (+ zgui-text-parley)
            │
            │  fragments: geometry, hit regions, stacking order
            ▼
  paint                             zgui-paint
            │
            │  a display list: primitives, batches, vector passes, damage
            ▼
  the renderer                      zgui-render (contract), zgui-render-wgpu (+ vector rasteriser)
            │
            ▼
  a surface                         zgui-platform (contract), zgui-platform-wayland /
                                    -winit / -headless
```

The arrows are one-way. A stage never reaches back up: layout does not mutate the document, paint
does not consult the cascade, and the renderer knows nothing about elements. This is what makes each
stage testable on its own, and it is why every horizontal edge in the diagram is a value type rather
than a shared object.

## The frame

`zgui-runtime` is the crate that runs the diagram. One frame has these phases, in this order:

1. **Accept work.** The runtime dispatches queued platform events, fires due timers, advances
   scrolling and gestures, reconfigures the surface, and samples running animations. Between two
   queued events, it settles reactive work so the second event sees the document changes from the
   first event.
2. **Reactive settle.** Listener bodies, timers, and completed tasks write signals. Writing marks
   observers; it does not run them. The frame flushes the reactive graph, calls the script-engine
   checkpoint, and carries out queued view commands.
3. **Restyle.** The style engine walks only the elements that owe a restyle. It computes their
   styles and turns each change into obligations for the stages below. For example, a colour change
   causes repaint damage and a width change causes relayout.
4. **Layout.** The runtime patches changed box-tree regions and runs taffy over the dirty region. It
   shapes or re-breaks paragraphs only when their content or available width changes. It then
   delivers geometry observations. An observation can cause at most two additional
   flush-restyle-layout passes. The runtime also updates hit targets under stationary pointers and
   plans the caret.
5. **Paint.** The stacking-order walk replays the display-list recording for unchanged fragments.
   It re-emits changed fragments and culls them against the damage rectangles.
6. **Draw.** The renderer receives the finished display list and the damage set. It composes into a
   target that it keeps between frames. Pixels outside the damage keep their values from the
   previous frame.
7. **Publish and schedule.** The runtime publishes the accessibility update, recycles frame-local
   document data, applies renderer pacing information, and calls the frame probe with the final
   window state.

Then the loop parks. A queued platform event or a wake can request a frame. The loop can also wake
at the earliest deadline for an animation, a timer, a held gesture, a paced resize, a caret blink,
a held presentation, or a renderer retry. Repeated requests are coalesced.

## Crates, by layer

Dependencies can point within a layer or to a lower layer. They do not point to a higher layer. The
layer of a crate is stated at the top of its manifest, and the manifest graph is checked
mechanically.

| Layer | Crates | What the layer is |
|---|---|---|
| L0 foundation | `zgui-geom`, `zgui-color`, `zgui-arena`, `zgui-interned`, `zgui-bits`, `zgui-profile` | Coordinate spaces, colour, storage whose addresses hold still, interned names, invalidation bits, counters. No policy. |
| L1 contracts | `zgui-vocab`, `zgui-css`, `zgui-scene`, `zgui-render`, `zgui-atlas`, `zgui-platform`, `zgui-reactive` | The traits and value types the rest of the tree agrees on. Nothing here implements anything replaceable. |
| L2 backends | `zgui-render-wgpu`, `zgui-render-vector-vello`, `zgui-render-vector-coverage`, `zgui-platform-wayland`, `zgui-platform-winit`, `zgui-platform-headless` | Implementations of L1 contracts. Every one of them is replaceable, and every one of them has a sibling. |
| L3 document | `zgui-dom` | One arena of nodes, safe to read from many threads while the cascade runs over it. |
| L4 engines | `zgui-style`, `zgui-layout`, `zgui-text`, `zgui-text-style`, `zgui-text-parley`, `zgui-paint`, `zgui-svg` | The stages that turn a document into a display list. |
| L5 systems | `zgui-input`, `zgui-scroll`, `zgui-a11y`, `zgui-anim`, `zgui-edit` | Behaviour over a laid-out document: hit testing and dispatch, scrolling, the accessibility projection, transitions and animations, text editing. |
| L6 frontend | `zgui-view`, `zgui-view-dom`, `zgui-view-macro`, `zgui-elements` | How an interface is described, and the three seams it is described against. |
| L7 runtime and tooling | `zgui-runtime`, `zgui`, `zgui-testkit-scene`, `zgui-testkit-view`, `zgui-conformance` | The frame pipeline, the umbrella crate an application depends on, and the instruments. |
| L8 product | `zgui-ui`, `zgui-ui-primitives`, `zgui-ui-tokens`, `zgui-ui-icons`, `zgui-devtools`, `zgui-examples`, `zgui-bench` | The component library, the inspector, the worked applications, and the measurement harness. Ordinary consumers of the public API. |

`zgui` is the crate an application depends on. Everything else is reachable through it, and an
application that only wants to describe an interface never names any of the others.

The unpublished `probe` crate is outside these product layers. It is a compile canary that verifies
that all pinned external engines can build together.

## The seams

A seam is a trait with an implementation on each side and no third party allowed to reach across it.
These are the seams a consumer is most likely to meet.

| Seam | Crate | What plugs in |
|---|---|---|
| `Renderer` | `zgui-render` | A device that puts a display list on a screen. |
| `VectorRaster` | `zgui-render` | A path rasteriser the renderer composites the output of. |
| `Surface`, `AppHandler`, `Clock`, `Waker`, `Clipboard` | `zgui-platform` | A windowing system, or the absence of one. |
| `Dom`, `ViewHost`, `EventSink` | `zgui-view` | The node tree, the engine that laid it out, and where a handler's commands go. |
| `SheetLoader`, `ReplacedContent`, `LinkResolver`, `PresentationalHints` | `zgui-dom` | The four things a document language brings that a document core cannot define for itself. |
| `FontSource`, `FontMetricsSource`, `ParagraphShaper`, `GlyphRaster` | `zgui-text` | A font engine, or a fixed-metrics stand-in for tests. |
| `HostBinding` | `zgui-runtime` | An embedded script engine's event, frame, and shutdown hooks. |
| `FrameProbe` | `zgui-runtime` | A reader of the window as each frame left it. |
| `MeasureContent` | `zgui-layout` | The measurement of anything layout cannot size itself. |

Most of these seams have two in-tree implementations. `HostBinding` is the exception: zgui provides
the no-op binding, and a downstream script engine provides the active binding.

## Windows

An application has as many windows as it opens. `use_windows().open(options, view)` opens one from
anywhere the application runs — a listener, an effect, a callback — and answers with a
`WindowHandle` before the window exists; `use_window()` resolves the window the calling code is
running *in*, the same way `set_timeout` resolves the host it schedules against.

Four things follow from where the state lives.

**A window is a document.** Each one gets an identity of its own, and every node handle carries the
identity of the document it was minted in — so a handle from one window cannot resolve inside
another, and the assertion that says so is a type-level one rather than a runtime check.

**A context belongs to whoever provided it.** A window's own scope is a child of the application's,
so what a component provides is that window's and what the application provides above them is
everyone's. Signals belong to neither: one written in any window is read in all of them, which is
what makes shared state need no plumbing.

**Every window draws on one graphics device.** `SharedGraphics` opens the device with the first
window and hands the rest a renderer on the same one, sharing the compiled pipelines. What is
per-window is what is sized to a window: the swap chain, the composed target, the frame's buffers.

**Asking for something a desktop cannot do is not an error.** Every operation on a window that has
closed does nothing, and so does every operation this desktop will not carry out — placing a window
on Wayland, resizing from an edge on macOS. An application asks once and runs everywhere;
`capabilities()` is there for one that would rather hide an affordance than offer a dead one.

## The three ideas the design turns on

**Invalidation is a lattice, not a flag.** A node carries what it *owes* — restyle, relayout,
repaint, and their subtree halves — and each stage retires exactly the obligations it serviced as it
walks. A stage never has to ask "did anything change?" globally; it is handed the set.

**A description is a value.** The display list is a value with no renderer in it. A computed style
is a shared pointer two elements that cascaded alike will hold the same copy of. A fragment tree is
data. Everything crossing a stage boundary can be printed, compared and asserted on with no device
and no window, which is why most of this framework's tests run headless.

**Cost is proportional to change.** A signal write invalidates precisely its observers. A restyle
visits the elements that owe one. A frame emits the primitives the damage rectangles reach. There is
no per-frame full traversal anywhere in the pipeline, and the absence of one is measured rather than
assumed.

## Where to go next

- [The layering rules](layering.md) — what may depend on what, and why the rule is worth the cost.
- [The styling model](styling.md) — sheets, origins, the cascade, and what a style change costs.
- [The reactive model](reactivity.md) — signals, owners, the flush, and the three `Send` escapes.
- [Doing something that takes time](async.md) — tasks, worker threads, cancellation, and tokio.
- [Writing a `Renderer`](renderer.md) — the contract, damage, and what a second renderer must not do.
- [Building a browser on zgui](browser.md) — the extension points, in the order a consumer meets them.
