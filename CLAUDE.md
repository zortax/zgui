# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`zgui` is a retained UI framework for native Rust applications: components, fine-grained reactive
signals, CSS styling (Stylo), layout (cephal), text (Parley), and a self-owned GPU pipeline (wgpu +
Vello). No browser, no HTML, no JavaScript. The API is unstable and the project is experimental.
`CONTRIBUTING.md` is the authoritative process reference; this file is the fast-path summary.

## Toolchain

- **Development is nightly-only.** `rust-toolchain.toml` pins `nightly-2026-04-16` with rustfmt,
  clippy, rust-src and miri. The pin moves in its own commit, CI-green first.
- **Published crates also build on stable Rust 1.90.** The `msrv` CI job type-checks them on that
  toolchain, so code in them must not need nightly. The unpublished console stack does need it
  (`zgui-xkb`).
- `vendor/fontique` is a patched `fontique`, wired in through `[patch.crates-io]` so that parley
  resolves against the same copy. Local changes are marked `ZGUI-PATCH:`, and `vendor/README.md`
  records every change and why. It is excluded from the workspace and from rustfmt
  (`rustfmt.toml`). Never reformat it.
- `.cargo/config.toml` aliases `cargo xtask` to run the `xtask` package.

## Commands

The definition of done is `cargo xtask ci` green, on the pinned toolchain, from a clean checkout.
GitHub runs `cargo xtask ci --hosted`, which skips the gates whose bands are absolute numbers from
the reference machine: `perf`, `tails`, `resize`, `workloads`, `verify` and `growth`.

The daily steps (each is also a step of `ci`):

```
cargo fmt --all -- --check
cargo xtask lint                         # clippy, every target, both profiles
cargo build --workspace
cargo test --workspace
cargo test --workspace --doc
cargo doc --workspace --no-deps           # ci sets RUSTDOCFLAGS=-D warnings
```

Run one crate's tests, one test, or one integration-test file:

```
cargo test -p zgui-layout
cargo test -p zgui-layout box_tree
cargo test -p zgui-layout --test box_tree
```

Run an example (sources live in the workspace-root `examples/`; `zgui-examples` declares one
`[[example]]` per file):

```
cargo run -p zgui-examples --example counter
cargo build -p zgui-examples --features drm --example tty   # bare Linux console; see below
```

The ledger checks (manifest graph, layering, versions, attribution, unsafe, counters, …):

```
cargo xtask ledger              # all checks
cargo xtask ledger layers       # one check, by name: engines, unsafe, attribution, versions,
                                # pinned, layers, ignored, counters, skips, clock, mutation,
                                # inert, tag-syntax
cargo xtask ledger --self-test  # prove each check fails on its planted fixture
```

Create a workspace member that already passes the ledgers:

```
cargo xtask new-crate zgui-foo --layer L4
```

`cargo xtask help` lists every gate with a one-line description. Each gate runs on its own too:
`cargo xtask budget`, `skips`, `growth`, `verify`, `hits`, `a11y-geom`, `cadence`, `workloads`,
`resize`, `docs`, `release`, `tsan`, `perf`, …

### Test runs that need more than `cargo test`

The default workspace run does not enable `loom`, and it runs unoptimised. Run the following when
you change what they cover:

```
cargo test -p zgui-bits --features loom                    # DirtyCell interleavings
cargo test -p zgui-profile --release --features counters   # counter-based budget assertions
cargo miri test -p zgui-arena                              # arena aliasing
cargo miri test -p zgui-dom --test cells                   # document aliasing; see ci.yml for flags
cargo xtask tsan                                           # threaded targets under ThreadSanitizer
```

Tests that need a GPU, a window or a compositor probe for one. When the facility is absent they
print the reason on stderr and return. They are never `#[ignore]`d, so a green run on a machine
without a device is not evidence that they passed.

The experimental console stack (`zgui-console`, `zgui-platform-drm`, `zgui-drm`, `zgui-evdev`,
`zgui-libinput`, `zgui-seat`, `zgui-xkb`) is unpublished and outside the umbrella crate. A build
needs libclang and the kernel and libc headers for bindgen; at run time the stack opens
libxkbcommon, libseat and libinput.

## Architecture

### One retained pipeline, one-way arrows

A frame flows top-down through retained data; no stage reaches back up (layout never mutates the
document, paint never consults the cascade, the renderer knows nothing about elements):

```
components + signals   (zgui-view, zgui-view-macro, zgui-elements, zgui-view-dom, zgui-reactive)
        │  create/update nodes through the Dom seam
   document            (zgui-dom)
        │  invalidation as obligations on nodes
   cascade             (zgui-style over zgui-css)
        │  computed styles + the damage they imply
   box tree + layout   (zgui-layout over cephal, zgui-text + zgui-text-parley)
        │  fragments: geometry, hit regions, stacking order
   paint               (zgui-paint)
        │  display list: primitives, batches, vector passes, damage
   renderer            (zgui-render contract; zgui-render-wgpu + a vector rasteriser)
        │
   surface             (zgui-platform contract; -winit / -wayland / -headless / -drm)
```

`zgui-runtime` drives the frame in this order: accept work → reactive settle → restyle → layout →
paint → draw → publish and schedule. Invalidation is a **lattice of obligations** (restyle /
relayout / repaint and their subtree halves), not a global "did anything change?" flag; each stage
retires exactly what it serviced. Cost is proportional to change, and the absence of a per-frame
full traversal is measured by the `growth` and `workloads` gates.

`zgui` (L7) is the umbrella crate an application depends on. It re-exports the lower crates as
modules (`zgui::view`, `zgui::elements`, `zgui::canvas`, `zgui::platform`, …), so an application
that only describes an interface names nothing else.

### Layers

The crates under `crates/` sit in nine layers, L0–L8. **Dependencies never point to a higher
layer.** Every crate except the unpublished `probe` canary declares its layer in a
`# L<n> — <name>.` header at the top of its manifest. `cargo xtask ledger layers` reads that header
and checks the real manifest graph; `ledger engines` checks the named edges below.
`docs/guide/layering.md` states the rule in full and lists every crate.

| Layer | Contents |
|---|---|
| L0 foundation | `zgui-geom`, `-color`, `-arena`, `-interned`, `-bits`, `-profile` |
| L1 contracts | `zgui-vocab`, `-css`, `-scene`, `-render`, `-atlas`, `-platform`, `-reactive`, `-image`, `-wgsl` (shader source) |
| L2 backends | `zgui-render-wgpu`, `-render-vector-vello`, `-render-vector-coverage`, `-platform-winit`, `-platform-wayland`, `-platform-headless`, `-tokio`; the experimental console stack `-drm`, `-evdev`, `-libinput`, `-seat`, `-xkb`, `-platform-drm` |
| L3 document | `zgui-dom` |
| L4 engines | `zgui-style`, `-layout`, `-text`, `-text-style`, `-text-parley`, `-paint`, `-svg`, `-canvas` |
| L5 systems | `zgui-input`, `-scroll`, `-a11y`, `-anim`, `-edit` |
| L6 frontend | `zgui-view`, `-view-dom`, `-view-macro`, `-elements`, `-custom`, `-shader-macro` |
| L7 runtime + tooling | `zgui-runtime`, `zgui`, `-testkit-scene`, `-testkit-view`, `-conformance`, `-shader`, `-wgpu` (embedded surfaces) |
| L8 product | `zgui-ui`, `-ui-primitives`, `-ui-tokens`, `-ui-icons`, `-devtools`, `-examples`, `-bench`, `-console` |

When a lower crate seems to need something from above, the answer (in order of preference) is:
**invert it** (the lower crate defines a trait; the higher crate implements it and hands it in),
**push the type down** (e.g. `zgui-vocab` owns event/role/state types so the view layer and the
document share them without depending on each other), or **add a facade** (e.g. `zgui-css` is the
only crate that names the style engine, re-exporting what sixteen others read). Feature flags that
turn an upward edge on, and dev-dependencies that cross a seam "just for a test", both fail the
build.

### Engine firewall

Each external engine is reachable from a bounded, enumerable set of crates. The table is the
architecture and lives in `xtask/src/ledger/check/engines.rs`; `ledger engines` checks it. The main
rows:

- `stylo` and its satellites → `zgui-css`, `zgui-dom`, `zgui-style`
- `cephal` → `zgui-layout`
- `parley`, `fontique`, `harfrust`, `skrifa`, `swash` → `zgui-text-parley`
- `usvg` → `zgui-svg`
- `vello` → `zgui-render-vector-vello`
- `wgpu` → `zgui-render-wgpu`, `-render-vector-vello`, `-render-vector-coverage`,
  `-platform-winit`, `-platform-drm`
- `winit` (and its macOS/GTK satellites) → `zgui-platform-winit`
- the Wayland stack (`smithay-client-toolkit`, `wayland-client`, `calloop`, …) →
  `zgui-platform-wayland`
- `reactive_graph` and its satellites → `zgui-reactive`
- `tokio` → `zgui-tokio`
- `ash`, `rustix`, `bindgen` → the DRM and console crates
- `libloading` → `zgui-xkb`, `zgui-seat`, `zgui-libinput` (one crate per library opened at run time)

The `gpui` crate may never be named in any manifest. The reference implementation is material to
read, never to link.

Two dependency lists are pinned to exactly their members across all sections, development
dependencies included: `zgui-view` depends on exactly four crates (`zgui-geom`, `zgui-interned`,
`zgui-reactive`, `zgui-vocab`), and `zgui-vocab` on exactly three (`accesskit`, `zgui-geom`,
`zgui-interned`). No crate below the view layer may name `zgui-view`.

### Dependencies

Every external dependency is declared once in the root `[workspace.dependencies]` and inherited with
`foo.workspace = true`. A version requirement written in a member manifest is a ledger failure.
Every member inherits the workspace version and is released in lockstep; a path dependency on
another member must carry a `version = "0.1.0"` beside the path (a bare path publishes to no
registry).

## Conventions that will fail CI if missed

- Every crate carries `#![deny(missing_docs)]` and `#![forbid(unsafe_code)]` (workspace lints).
  `unsafe` is permitted only in the allowlist in `xtask/src/ledger/check/unsafe_code.rs` (storage,
  geometry, the document cells, the GPU crates and the FFI backends). Every `unsafe impl
  Sync`/`Send` states its reason in a `// SAFETY:` comment directly above; `ledger unsafe` checks
  both halves.
- Rustdoc on every public item, and it **stands alone**: never refer to a plan, phase, review,
  wave, milestone or proposal. Cross-reference other items instead. `cargo xtask docs` scans
  rustdoc and the guides for those references, for `§`, and for guide links that do not resolve.
- Small, focused, deeply nested modules. A file that would need section comments is split into
  submodules instead.
- `#[non_exhaustive]` on sets expected to grow without a workspace change (error enums, parity
  vocabulary). Deliberately **not** on enums of the framework's own content (`PrimitiveKind`,
  `Batch`, `Filter`, CSS keyword types): adding a member must be found by exhaustive matches.
- **Performance budgets may only be raised in a dedicated commit** that changes the number and
  records the reason in the test's own doc comment. Folding a raise into a feature commit is not
  permitted. Lowering needs no ceremony. `MAX_DAMAGE` (`zgui_bits`) changes are a measurement, not
  an argument; see CONTRIBUTING.md.
- No test is `#[ignore]`d to go green. A test that cannot run checks for what it needs, prints why
  to stderr, and returns.
- A new gate or ledger rule arrives with a fixture or mutation that makes it fail; ledger fixtures
  live under `xtask/fixtures/<check>/`.
- Adapted code (a shader function, a packing strategy) starts with a `// DERIVED-FROM:` header
  naming the source and licence, and gains a row in `NOTICE` under **Derived files**.
  `ledger attribution` checks that the headers and the table agree.

## Documentation style

- Keep documentation concise and clear; do not overexplain.
- Follow ASD-STE100 writing style.
- Avoid contrastive negations / negative parallelism unless absolutely necessary (do not use
  phrases like "not x, but y").

## Commits

Commit messages are concise and describe only what changed. **No co-author trailer, no references
to plans or discussions** — this overrides the harness default of appending a `Co-Authored-By` line.
Create a new commit rather than amending. Before destructive git operations, prefer a safer
alternative that achieves the same goal.

## Local-only paths (gitignored, not in a fresh checkout)

`docs/perf/`, `docs/planning/`, `docs/research/`, `docs/shots/` and `docs/component-library.md` are
gitignored developer artifacts. `CONTRIBUTING.md` references some of them, but they will not be
present in a clean checkout. Do not rely on them, and do not commit anything there.

The committed documentation is `docs/guide/` (`architecture.md`, `layering.md`, `styling.md`,
`reactivity.md`, `async.md`, `renderer.md`, `browser.md`, `canvas.md`), plus two generated files:
`docs/parity.md` (written by the conformance suite) and `docs/performance.md` (written by
`cargo xtask perf`). Commit a regenerated file with the change that moved it.
