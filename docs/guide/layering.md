# The layering rules

zgui has fifty-eight crates under `crates/`. Fifty-seven are in the layered product graph. The
unpublished `probe` crate is a compile canary for the pinned external engines and is not in a layer.
The graph must follow a machine-checked rule. This document gives that rule.

## The one rule

**Dependencies do not point to a higher layer.** The graph has nine layers. Each layered crate
declares its layer at the top of its manifest. A crate can depend on crates in the same layer or in a
lower layer. Same-layer dependencies are used when one stage in a layer supplies data to another
stage in that layer.

| Layer | Name | What belongs there |
|---|---|---|
| L0 | foundation | Geometry, colour, storage, interned names, invalidation bits, counters. Values and algorithms with no policy. |
| L1 | contracts | Traits and value types the rest of the tree agrees on. Nothing replaceable is implemented here. |
| L2 | backends | Implementations of L1 contracts against a real library — a graphics API, a windowing library. |
| L3 | document | The retained node tree. |
| L4 | engines | Cascade, layout, text, paint, vector documents. |
| L5 | systems | Input dispatch, scrolling, accessibility, animation, editing. |
| L6 | frontend | The view layer, its macros, and the element vocabulary. |
| L7 | runtime and tooling | The frame pipeline, the umbrella crate, the test instruments. |
| L8 | product | The component library, the inspector, the worked applications. |

`cargo xtask ledger layers` reads that declaration from every manifest and checks the real
dependencies against it. `cargo xtask ledger` also checks the manifest graph against the
implementation-phase order, and the named architectural edges in this guide.

## Why the rule earns its cost

The rule costs real ergonomics. A stage that wants one fact from a stage above it has to be handed
that fact rather than fetching it, which usually means a new parameter, sometimes a new trait, and
occasionally a value type invented to sit between two crates that must not see each other. That is
the price. What it buys:

**A backend's next major version reaches one crate.** A windowing library that renames its size
accessors, splits its lifecycle callbacks and removes its user-event payload touches every crate in
a framework that names it directly. Behind `zgui-platform` it touches `zgui-platform-winit` and
nothing else.

**Every stage is testable without the one below it.** The style engine is exercised over a document
with no layout engine. Layout is exercised with a fixed-metrics text source and no fonts. Paint is
exercised with no graphics device. The frame loop is exercised with no display server. None of that
is a special test mode; it is what the layering already made possible.

**A second implementation is an implementation, not a fork.** There are two platform backends, two
vector rasterisers, two text metric sources, and a capture renderer beside the GPU renderer. Each
of those exists because the boundary above it names no library.

## The five checks that enforce it

`cargo xtask ledger layers` enforces the rule above. The rule alone is not enough, because a
downward edge can still be the wrong edge, so four narrower checks run beside it.

### The declared layer

Every layered manifest opens its dependency list with a `# L<n> — …` header, and the check compares
that number with the header of every member the manifest names in `[dependencies]` or
`[build-dependencies]`. A member that declares no layer fails as well, because a member with no
layer is a member whose edges nothing compares, and a crate added tomorrow would leave the graph
through that gap.

Development dependencies are outside it. A crate drives its own subject through the stack above it —
a layout or paint stage against the golden-image harness, the windowing backend against a real frame
loop — and a test binary carries none of that into a consumer's build. The two seams where a
development edge does matter are the pinned lists below, which are checked across every section.

The header is committed, so a fork gets the same answer as this tree. `cargo xtask ledger topo` asks
a related question — a crate may not depend on one that arrives in a later implementation phase —
from a schedule that is local to a checkout, and it reports that it skipped when there is none.

### Engine naming

Every external engine is reachable from a bounded, enumerable set of crates, and the table is the
architecture. `stylo` and its satellites are named by `zgui-css`, `zgui-dom` and `zgui-style` and by
nothing else. `taffy` is named by `zgui-layout`. `parley` and its satellites by `zgui-text-parley`.
`vello` by `zgui-render-vector-vello`. `wgpu` by the three graphics crates and the windowing
backend. `winit` by `zgui-platform-winit`. `reactive_graph` by `zgui-reactive`.

The consequence is that replacing, patching or dropping any one of them is a change to a named set
of manifests that can be counted before the work starts.

Two rows of that table are easy to miss and are the reason the check exists at all. The style
engine's own length unit and the geometry type its container-query hook answers in appear in return
position of trait methods the document has to implement, so those two names are on the same row as
the engine — a firewall with two of its three names unpoliced is not a firewall.

### Two pinned dependency lists

`zgui-view`'s dependency list is *exactly* four crates: `zgui-geom`, `zgui-interned`,
`zgui-reactive`, `zgui-vocab`. The view layer is the thing a different backend gets substituted
underneath, and it can only be substituted underneath something that cannot see a document, a style
engine, a layout engine, a renderer or a window system. One added dependency and that silently stops
being true.

`zgui-vocab`'s is exactly three. It exists so that the view layer and the document can name the same
event kinds, roles and states without either depending on the other. A fourth dependency would make
it a bridge that carries something, which is a different crate with a different name.

Both lists are pinned across *all* sections, development dependencies included, because a test that
reached for a document would be evidence that the seam had stopped being sufficient.

### No upward edge into the view layer

No crate below `zgui-view` may name it. The temptation is event dispatch: resolving which listeners
an event reaches is a job for the layer that owns hit testing, and *calling* one is a job for the
layer above. A crate that named the view layer in order to call a handler directly would invert the
whole graph, and it would do so in a single line that reviews easily.

### One version, one release

Every crate in the workspace inherits one version from the workspace manifest and is released with
the others. There is no "this crate is at 0.3 and that one at 0.1". A consumer who has `zgui 0.1.0`
knows exactly which `zgui-render` is underneath it, and a compatibility matrix between our own
crates never comes into existence.

Two things have to hold for that, and neither is visible until a release is attempted, so both are
checked on every run: a member must inherit its version rather than spell one, and every
non-development dependency on another member must carry a version requirement beside its path. A
bare path is a fact about one checkout; a package whose dependencies are only paths publishes to no
registry at all.

## The published surface

Three rules govern what is visible and what may change.

**`#[non_exhaustive]` where the set is expected to grow without a change in this workspace.** That
means error enums, the reasons a frame did not reach the screen, the answers a host hook may give,
and the parity vocabulary. Adding a variant to one of those is a normal consequence of learning
something new about a device, a platform or a style engine, and it should not cost a consumer a
major version.

It deliberately does **not** mean the enums that enumerate the framework's own content —
`PrimitiveKind`, `Batch`, `Filter`, the CSS keyword types. Adding a member to one of those is a
change that has to be made everywhere in this tree at once, and the compiler refusing every
non-exhaustive match is precisely the mechanism that finds those places. Marking them
`#[non_exhaustive]` would trade that for a wildcard arm in our own crates, which is the wrong trade.

Traits are not covered by the attribute — Rust does not allow it there. A trait a consumer
*implements* stays extensible the other way: a method added later arrives with a default body, so
an existing implementation keeps compiling. `HostBinding` is the worked example; every one of its
methods has a do-nothing default.

**`#[doc(hidden)]` on anything that exists only so one crate can reach another.** The typestate
markers that turn a missing required prop into a named compile error, and the inference markers that
let one setter take a literal, a closure or a signal, are mechanism: nothing is written by hand
against them, and listing them in the documentation index makes the index worse. The macro-expansion
roots are the exception and stay visible, because a crate writing views without the umbrella crate
over it has to know they exist.

**`cargo semver-checks` against the previous release tag**, run as a gate. Before the first release
it reports that there is no baseline rather than passing quietly, because a gate that says "ok" when
it did not run is worse than no gate.

## Reading a manifest

Every manifest states its layer and its reason on the same line, and every non-obvious dependency
carries a comment saying what it is for:

```toml
[package]
name = "zgui-layout"
version.workspace = true

# L4 — engines. External dependencies are inherited: `foo.workspace = true`.
[dependencies]
taffy.workspace = true
zgui-css = { path = "../zgui-css", version = "0.1.0" }
```

Every external dependency is declared once, in `[workspace.dependencies]`, and inherited with
`foo.workspace = true`. A version, a feature set and a `default-features` decision therefore exist in
exactly one place in the tree. This is also checked.

## When the rule seems to be in the way

Three moves cover almost every case, in order of preference.

**Invert it.** The lower crate defines a trait; the higher crate implements it and hands it in. This
is how the document gets stylesheet loading, replaced-content sizing and link resolution without
knowing what a URL is. See [building a browser on zgui](browser.md).

**Push the type down.** Two crates that must not see each other can still name the same type if it
lives below both. This is what `zgui-vocab` is: event kinds, roles and interaction states, named
identically by the view layer and the document, owned by neither.

**Add a facade.** When many crates need one foreign type, one crate owns the edge and re-exports
what the others need. `zgui-css` is that for the style engine: sixteen crates read computed styles,
and exactly one of them names the engine.

What is *not* an answer is a feature flag that turns an upward edge on, or a development-only
dependency that reaches across a seam "just for a test". Both are checked, and both fail the build.
