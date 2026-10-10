# Vendored crates

## cephal 0.1.0 (commit 26f31e9)

A copy of `~/Projects/cephal` at its first commit, without the taffy differential harness
(`src/compat`, the `taffy-compat` feature and the git dependency it pulled in), the Chrome fixture
suite, the benches and the examples. The layout engine that replaces taffy; `parallel` and `stats`
stay off, the workspace drives its own pool through the `batches_help` seam. Every local change is
marked `ZGUI-PATCH:` in the source.

### Fixed-position overflow (`Style::item_is_fixed`)

cephal has no `position: fixed`; the workspace lays fixed boxes out as `Position::Absolute`. An
absolutely positioned child contributes to its containing block's scrollable overflow, a fixed one
is anchored to the viewport and contributes none. The style carries `item_is_fixed`, and the three
places that fold an absolute child's contribution into the container's overflow (`compute/block/
absolute.rs`, `compute/flex/absolute.rs`, `compute/grid/mod.rs`) record a zero rectangle for a
fixed child. The `a_wrapped_flex_claims_the_width_it_wrapped_to` fixture in
`crates/zgui-ui/tests/cycle_scroll.rs` and the scrim suite in `crates/zgui-ui/tests/scrim.rs`
hold the workspace to it.

### The host sees every answer (`LayoutTree::after_layout`)

`compute::compute_child_layout` calls a defaulted `after_layout(node, &inputs, &mut output,
cached)` on every answer it hands out, cached or computed, hidden answers excepted. The workspace
counts the answer and records the baselines the algorithms leave out — a container's last
baseline, a leaf's first — on the box, on every answer, so a result served from the cache carries
the same baselines a computed one does. `crates/zgui-layout/tests/{baseline,counters}.rs` pin it.

### `BlockContext::has_floats`

Whether any float was placed in the formatting context, read by the inline layer to decide
whether a line has to band around floats at all. `crates/zgui-layout/tests/floats.rs` pins it.

### `Debug` on the scheduler

`schedule::Scheduler` and its depth queue derive `Debug`, because the store that holds them does.

## fontique 0.11.0 (crates.io, upstream commit 033e0b0)

The font collection under parley, patched in through `[patch.crates-io]` so that parley resolves
against the same copy. Without the examples. Every local change is marked `ZGUI-PATCH:` in the
source.

### Query state kept across queries (`collection/query.rs`)

Upstream clears a collection's query state when a query starts and again when it ends, and parley
starts one query per paragraph. For a generic family such as `sans-serif`, which a system
collection resolves to some two hundred families, every paragraph then rebuilt that list, and
every digit, which parley shapes as a possible emoji, rebuilt it again with the emoji families
appended. The state now outlives the query:

- the current family list and the last three are kept with the families and faces they loaded,
  and a `set_families` call that repeats one of them reuses it;
- the fallback list is rebuilt only when its script and locale change;
- loaded faces are dropped in `matches_with` when the attributes changed since they were matched;
- a query that sets no families or fallbacks still matches none.

`Collection` counts a generation, raised by every mutating method and by a sync that copies a
shared clone's changes; a query that finds the state built against another generation clears it.
`crates/zgui-text-parley/tests/fonts.rs`
(`a_face_unregistered_after_shaping_is_not_drawn_with_again`) pins the invalidation.
