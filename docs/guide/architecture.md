# Architecture

This document describes what zgui is made of and how one frame gets from a person moving a pointer
to pixels on a screen. It is the map the other guides assume: [layering](layering.md) states the
rules the map obeys, [the styling model](styling.md), [the reactive model](reactivity.md) and
[writing a renderer](renderer.md) each take one region of it in detail, and [building a browser on
zgui](browser.md) names the places a consumer plugs into.

## The shape of it

zgui is a **retained** framework. Nothing is rebuilt per frame that did not change: not the
document, not the computed styles, not the box tree, not the shaped paragraphs, not the display
list, and not the pixels. Every stage keeps its result and is handed a description of what became
invalid.

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
  a surface                         zgui-platform (contract), zgui-platform-winit / -headless
```

The arrows are one-way. A stage never reaches back up: layout does not mutate the document, paint
does not consult the cascade, and the renderer knows nothing about elements. This is what makes each
stage testable on its own, and it is why every horizontal edge in the diagram is a value type rather
than a shared object.

## The frame

`zgui-runtime` is the crate that runs the diagram. One frame, in order:

1. **Input.** The platform reports an event. `zgui-input` resolves what was under the pointer from
   the previous frame's fragments, and builds the capture/target/bubble path. `zgui-runtime` calls
   the listeners on it.
2. **Reactive settle.** Listener bodies write signals. Writing marks observers; it does not run
   them. Once dispatch is finished, the frame flushes the reactive graph, and the effects that ran
   mutate the document through the same seam a view builds through.
3. **Restyle.** The style engine walks only the elements that owe a restyle, computes their styles,
   and turns what changed into obligations for the stages below — a colour change is repaint damage,
   a width change is relayout.
4. **Layout.** Box-tree patches are applied for the subtrees that changed, taffy is run over the
   dirty region, and paragraphs are shaped or re-broken only where their content or their available
   width moved. The result is a fragment tree.
5. **Paint.** The stacking-order walk replays the display-list recording for fragments that did not
   change and re-emits only those that did, culled against the damage rectangles.
6. **Draw.** The renderer is handed the finished display list and the damage set, and composes into
   a target it keeps between frames. Outside the damage, last frame's pixels are still correct and
   are not touched.
7. **Publish.** The accessibility tree is diffed and published, and the frame probe — the seam the
   inspector attaches to — is called once with the window as it was left.

Then the loop parks. It burns nothing until a document mutation, an input event, work completing on
another thread, or a deadline asks for another frame. Those four are the entire list, and a frame
that raises several requests inside itself costs one more frame, not several.

## Crates, by layer

Dependencies point strictly downward. The layer of a crate is stated at the top of its manifest and
checked mechanically.

| Layer | Crates | What the layer is |
|---|---|---|
| L0 foundation | `zgui-geom`, `zgui-color`, `zgui-arena`, `zgui-interned`, `zgui-bits`, `zgui-profile` | Coordinate spaces, colour, storage whose addresses hold still, interned names, invalidation bits, counters. No policy. |
| L1 contracts | `zgui-vocab`, `zgui-css`, `zgui-scene`, `zgui-render`, `zgui-atlas`, `zgui-platform`, `zgui-reactive` | The traits and value types the rest of the tree agrees on. Nothing here implements anything replaceable. |
| L2 backends | `zgui-render-wgpu`, `zgui-render-vector-vello`, `zgui-render-vector-coverage`, `zgui-platform-winit`, `zgui-platform-headless` | Implementations of L1 contracts. Every one of them is replaceable, and every one of them has a sibling. |
| L3 document | `zgui-dom` | One arena of nodes, safe to read from many threads while the cascade runs over it. |
| L4 engines | `zgui-style`, `zgui-layout`, `zgui-text`, `zgui-text-style`, `zgui-text-parley`, `zgui-paint`, `zgui-svg` | The stages that turn a document into a display list. |
| L5 systems | `zgui-input`, `zgui-scroll`, `zgui-a11y`, `zgui-anim`, `zgui-edit` | Behaviour over a laid-out document: hit testing and dispatch, scrolling, the accessibility projection, transitions and animations, text editing. |
| L6 frontend | `zgui-view`, `zgui-view-dom`, `zgui-view-macro`, `zgui-elements` | How an interface is described, and the three seams it is described against. |
| L7 runtime and tooling | `zgui-runtime`, `zgui`, `zgui-testkit-scene`, `zgui-testkit-view`, `zgui-conformance` | The frame pipeline, the umbrella crate an application depends on, and the instruments. |
| L8 product | `zgui-ui`, `zgui-ui-primitives`, `zgui-ui-tokens`, `zgui-ui-icons`, `zgui-devtools`, `zgui-examples` | The component library, the inspector, the worked applications. Ordinary consumers of the public API. |

`zgui` is the crate an application depends on. Everything else is reachable through it, and an
application that only wants to describe an interface never names any of the others.

## The seams

A seam is a trait with an implementation on each side and no third party allowed to reach across it.
The framework has twenty of them; these are the ones a consumer is most likely to meet.

| Seam | Crate | What plugs in |
|---|---|---|
| `Renderer` | `zgui-render` | A device that puts a display list on a screen. |
| `VectorRaster` | `zgui-render` | A path rasteriser the renderer composites the output of. |
| `Surface`, `AppHandler`, `Clock`, `Waker`, `Clipboard` | `zgui-platform` | A windowing system, or the absence of one. |
| `Dom`, `ViewHost`, `EventSink` | `zgui-view` | The node tree, the engine that laid it out, and where a handler's commands go. |
| `SheetLoader`, `ReplacedContent`, `LinkResolver`, `PresentationalHints` | `zgui-dom` | The four things a document language brings that a document core cannot define for itself. |
| `FontSource`, `FontMetricsSource`, `ParagraphShaper`, `GlyphRaster` | `zgui-text` | A font engine, or a fixed-metrics stand-in for tests. |
| `HostBinding` | `zgui-runtime` | An embedded script engine's three frame hooks. |
| `FrameProbe` | `zgui-runtime` | A reader of the window as each frame left it. |
| `MeasureContent` | `zgui-layout` | The measurement of anything layout cannot size itself. |

Each of these exists because there are two things behind it that we actually build. A trait with one
implementation is an indirection, not a boundary.

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
- [Writing a `Renderer`](renderer.md) — the contract, damage, and what a second renderer must not do.
- [Building a browser on zgui](browser.md) — the extension points, in the order a consumer meets them.
