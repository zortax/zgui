# zgui

zgui is a native Rust UI framework with a Leptos-style programming model and a
browser-engine-style rendering pipeline. Components create the interface once. Signals update the
parts that depend on them, and the renderer keeps everything it can reuse between frames.

It uses native windows and GPU rendering. It does not embed a browser or require HTML or
JavaScript.

> [!WARNING]
> zgui is experimental and pre-1.0. It currently requires the pinned nightly toolchain, is not
> published on crates.io, and changes frequently. CSS and component coverage are substantial but
> incomplete.

## Highlights

- **Fine-grained state:** signals, memos, stores, selectors, contexts, async resources, and effects
  built on Leptos' [`reactive_graph`](https://docs.rs/reactive_graph/0.2.14/reactive_graph/).
- **Build-once components:** `#[component]` and `view!` create retained nodes. Reactive closures
  update those nodes directly; there is no virtual DOM.
- **CSS:** selectors, cascade, inheritance, custom properties, pseudo-classes, media queries,
  animations, transforms, filters, flexbox, and grid.
- **Incremental rendering:** retained styles, layout boxes, shaped text, paint recordings, GPU
  resources, and final pixels, with invalidation passed between each stage.
- **Application components:** a shadcn-style component library with fields, menus, dialogs,
  overlays, tables, navigation, charts, virtual lists, and headless interaction primitives.
- **Native application support:** Windows, macOS, Linux, and headless operation, with
  accessibility, IME and text editing, clipboard, drag and drop, multiple windows, async work,
  custom window chrome, and system theme changes.
- **Custom content:** canvas drawing, SVG/vector content, trait-based custom elements, images, and
  embedded wgpu surfaces.
- **Replaceable backends and testable stages:** renderer, platform, text, and document seams;
  headless tests, an inspector, and measured work counters.

## Architecture in one picture

```text
components + reactive_graph signals
                │  precise node mutations
                ▼
        retained document
                │  restyle / relayout / repaint obligations
                ▼
    Stylo cascade → Taffy layout + Parley text
                │  fragments, hit regions, and damage
                ▼
        retained paint recordings
                │  finished scene + damage rectangles
                ▼
        wgpu renderer + Vello
                │
                ▼
       persistent rendered target
```

This resembles a small browser engine: document, cascade, layout, paint, composition. zgui uses
that proven separation for application UI, with a small native element vocabulary and explicit
backend interfaces. It does not implement the web platform, HTML, scripting, networking, or a
browser's compatibility surface.

## A small application

```rust
use zgui::prelude::*;

#[component]
fn Counter() -> impl IntoView {
    let count = RwSignal::new(0);

    view! {
        column(class = "counter") {
            text(class = "value") {{move || count.get().to_string()}}
            control(
                tabindex = Focus::Sequential,
                on:click = move |_| count.update(|n| *n += 1)
            ) {
                "Increment"
            }
        }
    }
}

const STYLE: &str = css!(
    ":root {
        display: flex;
        align-items: center;
        justify-content: center;
        background: #111318;
        color: #f2f4f8;
        font-family: sans-serif;
    }
    .counter { gap: 12px; align-items: center; }
    .value { font-size: 48px; font-weight: 700; }
    control { padding: 8px 16px; border-radius: 8px; background: #356df3; }
    control:hover { background: #477df8; }"
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_title("Counter")
        .with_size(360.0, 240.0)
        .with_stylesheet(STYLE)
        .run(|| view! { Counter() })
}
```

From this repository:

```console
cargo run -p zgui-examples --example counter
```

The closure around `count.get()` is a **reactive hole**. It subscribes to `count` and updates one
text node when the value changes. The component function itself does not run again.

## State and views

The model is close to [Leptos](https://github.com/leptos-rs/leptos) and SolidJS:

- A component runs once when it mounts.
- A signal records which reactive closures read it.
- Writing a signal schedules only those readers.
- `Memo` caches derived state. `Store` provides reactive fields. `Selector` is useful for sparse
  choices such as selection in a large list.
- `Show`, `For`, `Dynamic`, `Suspense`, and `Transition` update structural regions without
  rebuilding unrelated nodes.
- State and effects belong to an owner and are disposed when their mounted view is removed.

zgui wraps `reactive_graph` with a UI-thread executor and explicit ownership rules. Signal writes
can come from worker threads; their effects settle on the UI thread during a bounded frame flush.
Several writes in one event therefore become one settled document update.

This feels like a modern web framework because state is declarative, dependencies are automatic,
components compose, and appearance lives in CSS. The output is a native retained document instead
of a browser DOM.

## What makes it incremental

Incrementality continues after the signal graph:

1. **Reactive update.** A signal wakes its readers. Each binding writes only when its resulting
   value changed.
2. **Style.** Class, attribute, and interaction-state changes are filtered against selector
   dependencies. The style engine visits nodes that owe a restyle and translates the computed
   difference into layout, text, paint, or accessibility work.
3. **Layout and text.** Dirty box-tree regions are patched into the retained tree. Paragraphs are
   reshaped or rebroken when their content, style, or available width requires it.
4. **Paint.** Unchanged fragment recordings are replayed. Changed fragments are emitted and culled
   against damage rectangles.
5. **Draw.** The renderer updates a persistent target. Pixels outside the damage region keep their
   previous value.

A sparse update, such as changing one label or hovering one row, can stay sparse through the whole
pipeline. A wide update, such as resizing a layout, replacing a stylesheet, or changing an
inherited theme token, naturally reaches more nodes. Each stage still reuses unaffected data and
avoids work from unrelated stages. For example, a color change needs paint but no layout; scrolling
can translate retained content and draw only the newly exposed band.

This is a cost model, not a promise that every change is constant-time. Layout dependencies can
spread to parents and siblings, large damage can cover a window, and structural changes can be
wide. zgui exposes counters and headless probes so those boundaries can be measured.

### How the renderer works

Paint produces a finished, renderer-independent scene: ordered primitives, batches, vector passes,
resources, and damage. The default wgpu renderer executes that scene, batches compatible work,
keeps glyphs and images in atlases, and uses Vello for vector rasterization.

The important detail is the persistent render target. Desktop swap-chain images are transient, so
zgui first composes into a texture it owns across frames and then presents it. That makes partial
redraw valid: untouched pixels really are the pixels from the previous frame.

A browser engine follows the same broad progression from style to layout to display list to
composition. zgui's version is smaller, native-Rust-facing, and split by public value-based seams.
Most pipeline decisions can therefore be tested without a window or GPU.

## Components

`zgui-ui` currently covers 56 of the 61 components in the shadcn/ui `new-york-v4` reference used by
the port. It includes:

- buttons, fields, inputs, choices, forms, and validation;
- menus, comboboxes, commands, navigation, tabs, and sidebars;
- dialogs, drawers, sheets, popovers, tooltips, toasts, and collision-aware overlays;
- tables, data tables, charts, calendars, carousels, resizable panels, and virtual lists;
- unstyled primitives for focus scopes, roving focus, dismissal, anchored positioning, presence,
  collections, and controlled or uncontrolled state.

Components use the same public API as applications. Interaction state comes from the engine and is
styled with selectors such as `:hover`, `:focus-visible`, and `:checked`.

See [the component coverage record](https://zgui.zortax.de/docs/component-library) for exact gaps.

## Built with zgui

- [zdt](https://github.com/zortax/zdt) — a modal code editor with splits, a file tree, LSP, Git,
  integrated terminals, and coding-agent workflows.
- [zgui-editor](https://github.com/zortax/zgui-editor) — an embeddable editor component with large
  files, incremental tree-sitter highlighting, multiple cursors, and shared documents.
- [zgui-terminal](https://github.com/zortax/zgui-terminal) — an embeddable terminal emulator with
  pluggable transports, retained rows, scrollback, selection, and modern keyboard protocols.
- [Canora](https://github.com/zortax/canora) — a full Spotify client with local caching, playback,
  playlists, search, custom window chrome, and light/dark themes.

## Trying it

zgui is currently consumed from Git or a local checkout:

```toml
[dependencies]
zgui = { git = "https://github.com/zortax/zgui" }
```

Pin a commit for reproducible builds and use the repository's `rust-toolchain.toml`. Useful examples:

```console
cargo run -p zgui-examples --example todo
cargo run -p zgui-examples --example vector
cargo run -p zgui-examples --example surface
cargo run -p zgui-ui --example gallery
```

The `examples/` directory also covers async work, multiple windows, images, canvas drawing, custom
elements, resizing, and custom window chrome.

## Comparison with other Rust UI frameworks

These comparisons describe architecture and current project scope, not benchmark results. A
**sparse update** changes one small part of a large interface. A **wide update** changes many nodes
or a shared dependency. Layout can widen either kind of update in every framework.

The maturity notes reflect the upstream repositories and documentation on 2026-08-27.

<details>
<summary><strong>Iced</strong> — Elm architecture, broad widget set, coarse application updates</summary>

[Iced](https://github.com/iced-rs/iced) organizes an application as state, messages, `update`, and
`view`. The model is explicit and easy to trace: widgets produce messages, `update` changes state,
and `view` describes the result.

- **Programming model:** `view` runs after each batch of messages and rebuilds the widget
  description. Iced reconciles it with a persistent widget-state tree. The flow is predictable,
  but its natural update boundary is the application view, not the individual value read by one
  widget.
- **Fine-grained reactivity:** there is no automatic state-to-widget dependency graph. Iced 0.14's
  [reactive rendering](https://github.com/iced-rs/iced/pull/2662) avoids redundant redraws when an
  event did not change anything; it does not turn the Elm model into fine-grained signal updates.
  [`keyed`](https://docs.iced.rs/iced/widget/keyed/index.html), `lazy`, widget-state and layout
  caches, canvas caches, and renderer primitive caches preserve selected work.
- **Sparse and wide updates:** even a one-label model change enters `view` and tree reconciliation;
  layout and drawing then depend on which widget caches remain valid. This fixed broad work becomes
  noticeable in large desktop applications unless the developer deliberately keys, caches, and
  virtualizes expensive regions. Wide updates naturally rebuild more. zgui instead starts at the
  signal subscriber and invalidates only the dependent retained stages.
- **Components and maturity:** Iced has a large first-party set: text editing, tables, grids,
  scrollables, markdown, canvas, custom shaders, pane grids, overlays, and common controls. The
  [`iced_aw`](https://github.com/iced-rs/iced_aw) ecosystem adds more. It powers shipped software
  and COSMIC uses a maintained fork, while upstream still labels Iced experimental and evolves its
  API between releases. Current reports include a [100%-CPU responsive-image
  failure](https://github.com/iced-rs/iced/issues/3104), a [large canvas-image cache
  regression](https://github.com/iced-rs/iced/issues/3173), and [high idle GPU
  use](https://github.com/iced-rs/iced/issues/3331). These are specific open bugs, not a claim that
  every Iced application is slow, but they show that performance-sensitive paths still need care.
- **Styling and layout:** themes and per-widget style catalogs are Rust APIs. Each widget owns its
  layout strategy; rows, columns, containers, grids, tables, stacks, and custom widgets compose the
  page. There is no shared CSS cascade, selector system, or unified flex/grid layout engine.

Iced is a strong fit when an Elm-style message flow, a mature widget toolbox, web support, or a
software-rendering fallback matters most. For a wide, frequently changing desktop UI, its coarse
view boundary places more responsibility on the application author than zgui's dependency-driven
model.

</details>

<details>
<summary><strong>egui</strong> — immediate mode, minimal ceremony, excellent embedding</summary>

[egui](https://github.com/emilk/egui) is an immediate-mode UI library. Application code reads and
mutates state while declaring the visible controls on every requested frame.

- **Programming model:** UI code is ordinary control flow: call `ui.button(...)`, inspect its
  response, and update state immediately. There are no retained component handles or callback
  lifetimes for application code.
- **Fine-grained reactivity:** egui has no signal graph. It retains interaction state and caches
  behind stable IDs, while the application lays out and paints the active interface again for a
  requested frame. Idle applications can sleep, but an active frame is still a full UI pass.
- **Sparse and wide updates:** a one-label change and a broad state change both execute the active
  UI code. As egui's own [CPU usage notes](https://github.com/emilk/egui#cpu-usage) explain, full
  layout each frame can tax the CPU for complex interfaces and very large scroll regions. Deep
  desktop layouts, especially flexbox-like structures, therefore lose much of egui's reputation
  for speed. A fully retained engine has an inherent advantage here: unchanged nodes can keep
  style, layout, and paint results instead of reconstructing that work each frame. egui applications
  compensate with clipping, virtualization, and manual caches.
- **Components and maturity:** the official crates and community ecosystem cover common controls,
  text editing, tables, trees, plots, images, a date picker, and custom painting. egui is used in
  professional tools such as Rerun. Its README still describes the API as in flux and releases may
  break compatibility.
- **Styling and layout:** `Style`, `Visuals`, widget builders, and scoped overrides control the
  look. Class support is developing, but it is not a CSS cascade. Layout is immediate and centered
  on horizontal/vertical flows, panels, columns, grids, windows, and manual placement rather than
  CSS flexbox and grid.

egui's argument is simplicity. It is fast enough—and often the best trade—for debug tools,
game/engine overlays, and applications with a relatively small or simple UI. zgui is aimed at the
other end: large, flex-heavy desktop interfaces where retained layout and sparse updates pay for
their extra machinery.

</details>

<details>
<summary><strong>GPUI</strong> — explicit entities and caching, with coarse CPU-side updates</summary>

[GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) is Zed's hybrid immediate and
retained UI framework. State lives in framework-owned `Entity<T>` values and is accessed through
explicit application, window, and entity contexts.

- **Programming model:** entities communicate through observers, subscriptions, events, and
  `cx.notify()`. Views implement `Render` and produce elements. Low-level elements can own layout,
  input, and paint behavior directly. This gives framework authors a useful imperative escape
  hatch, but it is separate from—and does not require—the coarse entity notification model.
- **Fine-grained reactivity:** entity notification is explicit rather than derived from signal
  reads. Current GPUI can rerender an entity-backed view subtree, and
  [`cached(style)`](https://github.com/zed-industries/zed/blob/main/crates/gpui/src/view.rs) creates
  an explicit stronger boundary: it recycles that view's layout and paint until the entity is
  notified or its bounds, mask, or text style change. Unchanged cached siblings can therefore be
  reused, but the application must choose suitable entity and cache boundaries—and cached views
  need a definite externally supplied size.
- **Sparse and wide updates:** this caching is coarse in practice. Once a view boundary is
  invalidated, its element subtree is rebuilt; parent sizing and flex relationships can still make
  a small visual change trigger layout across a much larger tree. Small pixel damage therefore
  does not necessarily mean small CPU work. Uniform lists, custom elements, and carefully placed
  cached views can contain that cost, but they are application-level engineering rather than
  automatic per-property dependencies. As an interface becomes deeper and wider, performance
  increasingly depends on the application author manually finding the right cache boundaries and
  replacing general layout with specialized elements. zgui carries dependency and damage
  information through style, layout, paint, and composition instead.
- **Components and maturity:** GPUI has exceptional real-world validation through Zed. The public
  crate is still pre-1.0, frequently changes with Zed, and its own README says that examples and
  guides are still being expanded. GPUI supplies primitives rather than a comprehensive standalone
  application component library; Zed's internal `ui` crate and community projects such as
  [`gpui-component`](https://github.com/longbridge/gpui-component) fill that layer.
- **Styling and layout:** elements use a fluent, Tailwind-like Rust API with flex layout, absolute
  positioning, text, shadows, transforms, and custom paint. It has no stylesheet parser, selector
  matching, inheritance, or cascade.

GPUI is production-tested, but that should not be confused with an inherently efficient update
architecture. Zed is fast because it is carefully engineered: its application chrome is relatively
clean and simple, while difficult paths such as the editor and long lists use specialized elements
and caches. The framework still makes sparse CPU work surprisingly easy to widen, and its manual
caching model does not grant an offsetting capability that requires that coarseness. zgui provides
custom layout and painting, raw GPU surfaces, and a replaceable renderer while also deriving
dependencies automatically and retaining work through later pipeline stages.

</details>

<details>
<summary><strong>Floem</strong> — the closest fine-grained native programming model</summary>

[Floem](https://github.com/lapce/floem) also constructs its main view tree once and updates it with
signals. Its reactive API is inspired by Leptos, while the implementation is Floem's own.

- **Programming model:** views compose as Rust values. Signals and effects update view state;
  dynamic stacks and views handle structural changes. This is the closest match to zgui at the
  application layer.
- **Fine-grained reactivity:** a derived label or style subscribes to the signals it reads, so a
  sparse write can update one retained view. zgui uses Leptos' current `reactive_graph` and extends
  the same idea through selector filtering, staged invalidation, retained paint recordings, and a
  persistent rendered target.
- **Renderer and actual incrementality:** Floem 0.2 defaults to its
  [Vger backend](https://github.com/lapce/floem/blob/main/Cargo.toml); Vello and Skia are optional,
  with tiny-skia available as a CPU fallback. The current window path begins a renderer frame,
  fills the full target, and [traverses the paint
  tree](https://github.com/lapce/floem/blob/main/src/window/handle.rs) before presenting. Vger keeps
  useful GPU resources such as glyphs and images, but this is not a persistent tiled compositor
  that redraws only damaged pixels. Floem's retained view tree and dirty style/layout phases can
  save substantial CPU work before painting; the final paint is currently window-wide.
- **Sparse and wide updates:** a sparse signal write reaches only its subscribers, after which
  dirty style and Taffy layout work propagate as required. A wide signal or structural change
  reaches more views; layout dependencies can widen either kind of update. Both frameworks offer
  virtual lists. zgui additionally retains paint recordings and the rendered target, so sparse
  updates stay sparse later in the pipeline instead of rebuilding the window's draw stream.
- **Components and maturity:** Floem includes common controls, text input and editing, dropdowns,
  lists, rich text, tabs, tooltips, a full editor view, and virtual stacks/lists. Its composite
  application-widget layer is smaller than zgui-ui's current menus/dialogs/forms/data-view set.
  Upstream describes Floem as still maturing on its way to v1.
- **Styling and layout:** a typed Rust `Style` API supports style classes, themes, responsive rules,
  transitions, and keyframe animations. Taffy supplies flexbox and grid. It does not parse CSS or
  implement the browser cascade and selector model.

Floem is the most direct alternative when build-once views and signals are the priority, especially
if typed Rust styling is preferable to CSS. Its application-side incrementality is close to zgui;
the largest architectural difference is how far that incrementality continues into painting and
composition.

</details>

<details>
<summary><strong>Dioxus Native / Blitz</strong> — Dioxus components over a native HTML/CSS engine</summary>

[Dioxus Native](https://github.com/DioxusLabs/blitz) renders a Dioxus virtual DOM through Blitz
instead of a system webview. Blitz and zgui share several building blocks: Stylo for CSS, Taffy for
box layout, Parley for text, AccessKit for accessibility, and a GPU renderer.

- **Programming model:** components return RSX. A hook or signal change reruns the subscribing
  component, Dioxus diffs its virtual DOM, and Blitz applies the resulting mutations to a retained
  native document. zgui's reactive closures bypass component reruns and virtual-DOM reconciliation.
- **Fine-grained reactivity:** Dioxus tracks which components read a signal. The update unit is
  normally that component and its generated VDOM, rather than an individual text/class/style
  binding. Small components keep this cost local.
- **Sparse and wide updates:** a sparse change reruns and diffs the affected component, then enters
  Blitz's style/layout/paint pipeline as DOM mutations. Wide changes rerun many components or a
  large component and create a wider mutation set. zgui starts with smaller binding-level writes;
  both engines must still pay for downstream CSS and layout dependencies.
- **Components and maturity:** the wider Dioxus ecosystem includes routing, full-stack tools,
  assets, accessible primitives, and a shadcn-style
  [`dioxus-components`](https://github.com/DioxusLabs/dioxus-components) library. Compatibility in
  Native also depends on Blitz's element, behavior, and CSS coverage. Dioxus' web and system-webview
  renderers are the mature paths; Blitz currently describes itself as beta and suitable for early
  adopters willing to accept missing features and bugs.
- **Styling and layout:** the authoring model is HTML and CSS, including selectors, custom
  properties, media queries, flexbox, grid, tables, block/inline layout, and form controls as Blitz
  implements them. This is the closest styling model to zgui. Dioxus Native targets HTML/CSS
  compatibility; zgui uses a smaller application-specific element vocabulary and does not carry an
  HTML parser or web-compatibility goal.

Dioxus Native is attractive when sharing Dioxus components and skills across web, webview, and
native renderers matters. zgui is more specialized around native application UI and binding-level
incrementality.

</details>

## Documentation

- [Architecture](https://zgui.zortax.de/docs/architecture/overview) — the frame pipeline, crate
  layers, and backend seams
- [Reactivity](https://zgui.zortax.de/docs/architecture/reactive-internals) — signals, owners,
  flushing, and worker-thread boundaries
- [Styling](https://zgui.zortax.de/docs/learn/styling) — CSS, the cascade, and style invalidation
- [Renderer](https://zgui.zortax.de/docs/architecture/renderer) — scenes, damage, persistent
  targets, and renderer contracts
- [Async work](https://zgui.zortax.de/docs/learn/async-and-timers) — UI tasks, background work,
  cancellation, and Tokio
- [CSS parity](docs/parity.md) — generated, test-backed property coverage
- [Performance](docs/performance.md) — generated counters, timings, and regression bands

## License

[Apache-2.0](LICENSE)
