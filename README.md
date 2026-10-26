<p align="center">
  <img src="https://github.com/user-attachments/assets/1f8f36b1-119f-43bd-95d3-40b2d4804ea6" alt="Applications built with zgui: a Kubernetes client, a music player, a voice chat, a code editor and a mind map" width="100%" />

</p>

<h1 align="center">zgui</h1>

<p align="center">
  A native Rust UI framework with fine-grained reactivity, real CSS and a retained GPU renderer.
</p>

<p align="center">
  <a href="https://zgui.zortax.de">Documentation</a> ·
  <a href="https://zgui.zortax.de/docs/getting-started/installation">Getting started</a> ·
  <a href="https://zgui.zortax.de/docs/component-library/components">Components</a> ·
  <a href="https://discord.gg/E6SDmyRRmd">Discord</a>
</p>

zgui combines a Leptos-style programming model with a browser-engine-style rendering pipeline.
Components run once. Signals update only the parts of the interface that read them, and every stage
after that (style, layout, paint and the final pixels) keeps the work it can reuse. Windows are
native and drawn with wgpu. There is no webview, HTML or JavaScript.

> [!WARNING]
> zgui is experimental and changes often. Expect breaking changes between 0.x releases.

## Features

- **Fine-grained state.** Signals, memos, stores, resources and effects from Leptos'
  [`reactive_graph`](https://docs.rs/reactive_graph). There is no virtual DOM.
- **CSS.** Selectors, the cascade, custom properties, media queries, flexbox, grid, transitions,
  animations, transforms and filters, through Stylo.
- **Incremental rendering.** Style, layout, text shaping, paint recordings and pixels are retained
  and invalidated separately, so a small change stays small.
- **Components and themes.** A shadcn-style library of controls, overlays, menus, forms, tables
  and charts, themed with design tokens.
- **A complete native platform.** Windows, macOS, Linux (Wayland and X11) and headless, with
  accessibility, IME, clipboard, drag and drop, multiple windows and custom window chrome.
- **Custom content.** Canvas drawing, SVG, custom elements, application shaders and embedded wgpu
  surfaces.

## Example

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
    ".counter { gap: 12px; align-items: center; }
    .value { font-size: 48px; font-weight: 700; }
    control { padding: 8px 16px; border-radius: 8px; background: #356df3; }
    control:hover { background: #477df8; }"
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_title("Counter")
        .with_stylesheet(STYLE)
        .run(|| view! { Counter() })
}
```

`Counter` runs once. The closure around `count.get()` is a **reactive hole**: it subscribes to
`count` and updates one text node when the value changes.

A slightly larger counter is in [`examples/counter.rs`](https://github.com/zortax/zgui/blob/main/examples/counter.rs):

```console
cargo run -p zgui-examples --example counter
```

## Components and theming

<p align="center">
  <img src="https://github.com/user-attachments/assets/a6acc01a-ef0a-41b8-8211-201eca86aedf" alt="A dashboard built from zgui-ui components, in light and dark" width="100%" />
</p>

`zgui-ui` ports most of [shadcn/ui](https://ui.shadcn.com): buttons, fields, selects, menus,
dialogs, popovers, toasts, tabs, sidebars, tables, charts, calendars and more. Headless primitives
for focus, dismissal, anchored positioning and roving selection are available on their own in
`zgui-ui-primitives`.

```rust
use zgui::prelude::*;
use zgui_ui::prelude::*;

#[component]
fn Deploy() -> impl IntoView {
    let branch = RwSignal::new_local("main".to_owned());
    let migrate = RwSignal::new_local(true);

    view! {
        Card {
            CardHeader {
                CardTitle {"Deploy storefront"}
                CardDescription {"Ships the branch to production."}
            }
            CardContent {
                Input(value = branch, label = "Branch")
                Switch(checked = migrate, a11y:label = "Run migrations")
            }
            CardFooter {
                Button(variant = ButtonVariant::Outline) {"Preview"}
                Button {"Deploy"}
            }
        }
    }
}
```

Every component reads its look from design tokens: colors, radii, spacing, shadows, type and
motion. A theme sets some or all of them, and `ThemeProvider` applies it to a window or a subtree.
Changing a theme at runtime is a signal write.

<p align="center">
  <img src="https://github.com/user-attachments/assets/e8548755-8e54-437a-ae86-01fab631b711" alt="The same panel in six themes" width="100%" />
</p>

```rust
use zgui_ui_tokens::prelude::*;

let mocha = Theme::dark().with_css(
    "--zui-color-background: #1e1e2e;
     --zui-color-primary: #cba6f7;
     --zui-radius-base: 8px;
     --zui-space-base: 3.5px;
     --zui-shadow-sm: none;
     --zui-type-family-sans: 'Readex Pro';",
);

view! {
    ThemeProvider(dark = mocha, scheme = ColorScheme::Dark) {
        Deploy()
    }
}
```

See the [component list](https://zgui.zortax.de/docs/component-library/components) and
[tokens and theming](https://zgui.zortax.de/docs/component-library/tokens-and-theming) for
everything that is available.

## How it works

```text
components + reactive_graph signals
                │  precise node mutations
                ▼
        retained document
                │  restyle / relayout / repaint
                ▼
    Stylo cascade → cephal layout + Parley text
                │  fragments, hit regions, damage
                ▼
        retained paint recordings
                │  scene + damage rectangles
                ▼
        wgpu renderer + Vello
                │
                ▼
       persistent render target
```

The stages follow a browser engine: document, cascade, layout, paint and composition. zgui uses a
small native element vocabulary instead of HTML, and each stage is a replaceable, testable crate.

Each stage passes only the work it owes to the next:

1. **Reactive update.** A signal wakes its readers. A binding writes to the document only when its
   value changes.
2. **Style.** Class, attribute and state changes are filtered against selector dependencies. Only
   affected nodes restyle, and the difference decides whether layout, text, paint or accessibility
   must run.
3. **Layout and text.** cephal, zgui's incremental layout engine, patches dirty regions of the box
   tree. Paragraphs are reshaped or rewrapped only when their content, style or width changes.
4. **Paint.** Unchanged fragments replay their recordings. Changed fragments are recorded again and
   culled against the damage rectangles.
5. **Draw.** The renderer composes into a target that lives across frames, so pixels outside the
   damage keep their previous value.

A color change needs paint and no layout. Scrolling moves retained content and draws only the band
that comes into view. Wide changes, such as a window resize or a new theme, still reach more nodes,
and zgui exposes counters and headless probes so you can measure where that boundary is.

Read more in the [architecture overview](https://zgui.zortax.de/docs/architecture/overview), the
[renderer](https://zgui.zortax.de/docs/architecture/renderer) and the
[cost model](https://zgui.zortax.de/docs/performance/cost-model).

## Built with zgui

- **holokube**: a Kubernetes desktop client.
- [zdt](https://github.com/zortax/zdt): a modal code editor with splits, a file tree, LSP, Git,
  terminals and coding-agent workflows.
- [Canora](https://github.com/zortax/canora): a Spotify client with playback, playlists, search,
  custom window chrome and light and dark themes.
- [moqspeak](https://github.com/Newspicel/moqspeak): voice chat and screen sharing over Media over
  QUIC.
- [zgui-flow](https://github.com/zortax/zgui-flow): node graphs and diagrams, in the style of React
  Flow.
- [zgui-editor](https://github.com/zortax/zgui-editor): an embeddable code editor with incremental
  tree-sitter highlighting, multiple cursors and large-file support.
- [zgui-terminal](https://github.com/zortax/zgui-terminal): an embeddable terminal emulator with
  scrollback, selection and modern keyboard protocols.

## Comparison with other Rust UI frameworks

These notes compare architecture and project scope. They are not benchmark results. A **sparse
update** changes one small part of a large interface. A **wide update** changes many nodes or a
shared dependency. In every framework, layout can make either kind of update wider.

The maturity notes reflect the upstream repositories and documentation on 2026-08-27.

<details>
<summary><strong>Iced</strong>: Elm architecture, broad widget set, application-level updates</summary>

[Iced](https://github.com/iced-rs/iced) organizes an application as state, messages, `update`
and `view`. The model is explicit and easy to trace: widgets produce messages, `update` changes
state, and `view` describes the result.

- **Programming model:** `view` runs after each batch of messages and rebuilds the widget
  description, which Iced reconciles with a persistent widget-state tree. The flow is predictable.
  Its natural update boundary is the whole application view.
- **Fine-grained reactivity:** Iced has no automatic state-to-widget dependency graph. Iced 0.14's
  [reactive rendering](https://github.com/iced-rs/iced/pull/2662) skips redraws when an event
  changed nothing. It does not make updates fine-grained.
  [`keyed`](https://docs.iced.rs/iced/widget/keyed/index.html), `lazy`, widget-state and layout
  caches, canvas caches and renderer primitive caches keep selected work.
- **Sparse and wide updates:** a one-label change still runs `view` and tree reconciliation.
  Layout and drawing then depend on which widget caches stay valid. In large desktop applications
  this fixed cost becomes visible unless the developer keys, caches and virtualizes expensive
  regions by hand. zgui starts at the signal's subscribers and invalidates only the retained
  stages that depend on them.
- **Components and maturity:** Iced has a large first-party set: text editing, tables, grids,
  scrollables, markdown, canvas, custom shaders, pane grids, overlays and common controls.
  [`iced_aw`](https://github.com/iced-rs/iced_aw) adds more. Iced powers shipped software and
  COSMIC uses a maintained fork. Upstream still calls it experimental, and the API changes between
  releases. Open reports include a [100% CPU responsive-image
  bug](https://github.com/iced-rs/iced/issues/3104), a [large canvas-image cache
  regression](https://github.com/iced-rs/iced/issues/3173) and [high idle GPU
  use](https://github.com/iced-rs/iced/issues/3331). These are specific bugs. They show that
  performance-sensitive paths still need care.
- **Styling and layout:** themes and per-widget style catalogs are Rust APIs. Each widget owns its
  layout strategy, and rows, columns, containers, grids, tables, stacks and custom widgets compose
  the page. There is no shared CSS cascade, selector system or unified flex and grid engine.

Iced is a strong choice when an Elm-style message flow, a mature widget set, web support or a
software-rendering fallback matter most. For a wide, frequently changing desktop interface, its
view-level boundary gives the application author more of the performance work than zgui's
dependency-driven model.

</details>

<details>
<summary><strong>egui</strong>: immediate mode, minimal ceremony, excellent embedding</summary>

[egui](https://github.com/emilk/egui) is an immediate-mode UI library. Application code reads and
changes state while it declares the visible controls, on every requested frame.

- **Programming model:** UI code is ordinary control flow: call `ui.button(...)`, check its
  response and update state at once. There are no retained component handles or callback lifetimes.
- **Fine-grained reactivity:** egui has no signal graph. It keeps interaction state and caches
  behind stable IDs, and lays out and paints the active interface again for each requested frame.
  Idle applications can sleep. An active frame is always a full UI pass.
- **Sparse and wide updates:** a one-label change and a broad state change both run the whole
  active UI. As egui's own [CPU usage notes](https://github.com/emilk/egui#cpu-usage) explain, full
  layout every frame can tax the CPU for complex interfaces and very large scroll regions. Deep
  desktop layouts, especially flexbox-like structures, lose much of egui's speed advantage. A
  retained engine keeps style, layout and paint for unchanged nodes and does not rebuild them each
  frame. egui applications compensate with clipping, virtualization and manual caches.
- **Components and maturity:** the official crates and the community cover common controls, text
  editing, tables, trees, plots, images, a date picker and custom painting. egui is used in
  professional tools such as Rerun. Its README still says the API is in flux and releases can break
  compatibility.
- **Styling and layout:** `Style`, `Visuals`, widget builders and scoped overrides control the
  look. Class support is in development, without a CSS cascade. Layout is immediate and built
  around horizontal and vertical flows, panels, columns, grids, windows and manual placement, with
  no CSS flexbox or grid.

egui's strength is simplicity. It is fast enough, and often the best trade, for debug tools, game
and engine overlays, and applications with a small or simple interface. zgui targets the other
end: large, flex-heavy desktop interfaces where retained layout and sparse updates pay for the
extra machinery.

</details>

<details>
<summary><strong>GPUI</strong>: explicit entities and caching, coarse CPU-side updates</summary>

[GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) is Zed's hybrid immediate and
retained UI framework. State lives in framework-owned `Entity<T>` values and is accessed through
explicit application, window and entity contexts.

- **Programming model:** entities communicate through observers, subscriptions, events and
  `cx.notify()`. Views implement `Render` and produce elements. Low-level elements can own their
  layout, input and paint, which gives framework authors a useful imperative escape hatch. That
  hatch is independent of the coarse entity notification model.
- **Fine-grained reactivity:** entity notification is explicit and is not derived from signal
  reads. GPUI can rerender an entity-backed view subtree, and
  [`cached(style)`](https://github.com/zed-industries/zed/blob/main/crates/gpui/src/view.rs) adds
  a stronger boundary: it recycles that view's layout and paint until the entity is notified or its
  bounds, mask or text style change. Unchanged cached siblings are reused. The application must
  choose the entity and cache boundaries, and a cached view needs a definite size from outside.
- **Sparse and wide updates:** in practice this caching is coarse. When a view boundary is
  invalidated, its element subtree is rebuilt, and parent sizing and flex relationships can turn a
  small visual change into layout across a much larger tree. Small pixel damage does not mean
  small CPU work. Uniform lists, custom elements and well-placed cached views contain that cost,
  but they are application engineering, not automatic per-property dependencies. As an interface
  grows deeper and wider, performance depends more and more on the author finding cache boundaries
  by hand and replacing general layout with specialized elements. zgui carries dependency and
  damage information through style, layout, paint and composition.
- **Components and maturity:** GPUI has exceptional real-world validation through Zed. The public
  crate is pre-1.0, changes often with Zed, and its README says examples and guides are still being
  expanded. GPUI supplies primitives. Zed's internal `ui` crate and community projects such as
  [`gpui-component`](https://github.com/longbridge/gpui-component) supply the component layer.
- **Styling and layout:** elements use a fluent, Tailwind-like Rust API with flex layout, absolute
  positioning, text, shadows, transforms and custom paint. There is no stylesheet parser, selector
  matching, inheritance or cascade.

GPUI is production-tested. That is different from an efficient update architecture. Zed is fast
because it is carefully engineered: its application chrome is simple, and difficult paths such as
the editor and long lists use specialized elements and caches. The framework makes sparse CPU work
easy to widen, and its manual caching model buys no capability that needs that coarseness. zgui
also offers custom layout and painting, raw GPU surfaces and a replaceable renderer, and in
addition derives dependencies automatically and retains work through the later pipeline stages.

</details>

<details>
<summary><strong>Floem</strong>: the closest fine-grained native programming model</summary>

[Floem](https://github.com/lapce/floem) also builds its main view tree once and updates it with
signals. Its reactive API is inspired by Leptos, with Floem's own implementation.

- **Programming model:** views compose as Rust values. Signals and effects update view state, and
  dynamic stacks and views handle structural changes. At the application layer this is the closest
  match to zgui.
- **Fine-grained reactivity:** a derived label or style subscribes to the signals it reads, so a
  sparse write can update one retained view. zgui uses Leptos' current `reactive_graph` and carries
  the same idea further with selector filtering, staged invalidation, retained paint recordings and
  a persistent render target.
- **Renderer and incrementality:** Floem 0.2 defaults to its
  [Vger backend](https://github.com/lapce/floem/blob/main/Cargo.toml), with optional Vello and Skia
  and a tiny-skia CPU fallback. The current window path begins a renderer frame, fills the full
  target and [traverses the paint
  tree](https://github.com/lapce/floem/blob/main/src/window/handle.rs) before it presents. Vger
  keeps GPU resources such as glyphs and images, but there is no persistent compositor that redraws
  only damaged pixels. Floem's retained view tree and dirty style and layout phases save a lot of
  CPU work before painting. The final paint is window-wide.
- **Sparse and wide updates:** a sparse signal write reaches only its subscribers, then dirty style
  and Taffy layout work spread as needed. A wide signal or structural change reaches more views,
  and layout dependencies can widen either kind. Both frameworks offer virtual lists. zgui also
  retains paint recordings and the rendered target, so sparse updates stay sparse to the end of the
  pipeline and the window's draw stream is not rebuilt.
- **Components and maturity:** Floem includes common controls, text input and editing, dropdowns,
  lists, rich text, tabs, tooltips, a full editor view and virtual stacks and lists. Its
  application-level component layer is smaller than zgui-ui's menus, dialogs, forms and data
  views. Upstream describes Floem as still maturing on its way to 1.0.
- **Styling and layout:** a typed Rust `Style` API supports style classes, themes, responsive
  rules, transitions and keyframe animations. Taffy supplies flexbox and grid. Floem does not parse
  CSS or implement the browser cascade and selector model.

Floem is the most direct alternative when build-once views and signals come first, especially if
you prefer typed Rust styling to CSS. Its application-side incrementality is close to zgui's. The
largest difference is how far that incrementality continues into painting and composition.

</details>

<details>
<summary><strong>Dioxus Native / Blitz</strong>: Dioxus components over a native HTML and CSS engine</summary>

[Dioxus Native](https://github.com/DioxusLabs/blitz) renders a Dioxus virtual DOM through Blitz
instead of a system webview. Blitz and zgui share several building blocks: Stylo for CSS, Parley
for text, AccessKit for accessibility and a GPU renderer. Blitz uses Taffy for layout, and zgui
uses cephal.

- **Programming model:** components return RSX. A hook or signal change reruns the subscribing
  component, Dioxus diffs its virtual DOM, and Blitz applies the resulting mutations to a retained
  native document. zgui's reactive closures skip component reruns and virtual-DOM reconciliation.
- **Fine-grained reactivity:** Dioxus tracks which components read a signal. The update unit is
  normally that component and its generated virtual DOM, one level above an individual text, class
  or style binding. Small components keep this cost local.
- **Sparse and wide updates:** a sparse change reruns and diffs the affected component, then enters
  Blitz's style, layout and paint pipeline as DOM mutations. Wide changes rerun many components, or
  one large component, and produce a wider mutation set. zgui starts with smaller binding-level
  writes. Both engines still pay for downstream CSS and layout dependencies.
- **Components and maturity:** the wider Dioxus ecosystem includes routing, full-stack tools,
  assets, accessible primitives and a shadcn-style
  [`dioxus-components`](https://github.com/DioxusLabs/dioxus-components) library. In Native,
  compatibility also depends on Blitz's element, behavior and CSS coverage. Dioxus' web and
  system-webview renderers are the mature paths. Blitz describes itself as beta and suitable for
  early adopters who accept missing features and bugs.
- **Styling and layout:** the authoring model is HTML and CSS, including selectors, custom
  properties, media queries, flexbox, grid, tables, block and inline layout, and form controls as
  Blitz implements them. This is the styling model closest to zgui. Dioxus Native aims for HTML
  and CSS compatibility. zgui uses a smaller, application-specific element vocabulary and has no
  HTML parser or web-compatibility goal.

Dioxus Native is attractive when you want to share Dioxus components and skills across web,
webview and native renderers. zgui specializes in native application UI and binding-level
incrementality.

</details>

## Getting started

Add zgui, and the component library if you want it, from crates.io:

```toml
[dependencies]
zgui = "0.1"
zgui-ui = "0.1"
zgui-ui-tokens = "0.1"
```

zgui builds on stable Rust 1.90 or later. On Linux, install `pkg-config`, `libxkbcommon`,
`libwayland`, the X11 libraries (`libx11`, `libxcb`, `libxcursor`, `libxi`, `libxrandr`) and
`fontconfig`.

The zgui repository patches fontique, the font collection under the text engine, so that it keeps
resolved font families from one paragraph to the next and, on Linux, keeps the system font
collection in a cache file between launches (`ZGUI_FONT_CACHE=off` disables it). A crates.io build
uses the released fontique, which shows the same text and resolves fonts more slowly. To use the
patched copy, add this to your application's `Cargo.toml`:

```toml
[patch.crates-io]
fontique = { git = "https://github.com/zortax/zgui", tag = "v0.1.0" }
```

To run the examples, clone the repository. Some examples to try:

```console
cargo run -p zgui-ui --example gallery
cargo run -p zgui-examples --example todo
cargo run -p zgui-examples --example vector
cargo run -p zgui-examples --example surface
```

The [`examples/`](https://github.com/zortax/zgui/tree/main/examples) directory also covers async
work, multiple windows, images, canvas drawing, custom elements, custom shaders and custom window
chrome. The [installation guide](https://zgui.zortax.de/docs/getting-started/installation) walks through a
first application.

## Community

Ask questions and share what you build on [Discord](https://discord.gg/E6SDmyRRmd). Read
[CONTRIBUTING.md](https://github.com/zortax/zgui/blob/main/CONTRIBUTING.md) before you open a pull request.

## License

[Apache-2.0](https://github.com/zortax/zgui/blob/main/LICENSE)
