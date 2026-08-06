# Vendored crates

## taffy 0.12.2

An unmodified copy of the published `taffy 0.12.2`, carrying three local changes.

### Fixed-position content size (`CoreStyle::is_fixed_position`)

Taffy has no `position: fixed`; this workspace lays fixed boxes out as `Position::Absolute`. That
conflated one thing that must differ: an absolutely positioned child contributes to its containing
block's scrollable overflow (CSS Overflow §3), while a fixed one is anchored to the viewport, does
not scroll with anything, and contributes none. The vendored copy adds a defaulted
`CoreStyle::is_fixed_position()` (false, so plain taffy behaviour is unchanged) and skips such
children when folding `absolute_content_size` in block, flexbox and grid layout. Without it the
viewport-sized overlay root — sized `100vw`/`100vh` so sheets and scrims cover the scrollbar
gutter — hands the page's scroller a sideways scrollbar the moment the page reserves one. The
`a_wrapped_flex_claims_the_width_it_wrapped_to` fixture in `crates/zgui-ui/tests/cycle_scroll.rs`
and the scrim suite in `crates/zgui-ui/tests/scrim.rs` hold the workspace to it.

### Auto margins and `justify-content`

One local fix in `src/compute/flexbox.rs` (`distribute_remaining_free_space`, marked `zgui local
patch`):

When a flex line has main-axis `auto` margins and positive free space, the spec hands the free
space to those margins *instead of* to `justify-content`
(<https://www.w3.org/TR/css-flexbox-1/#algo-main-align>). Upstream distributed the space into the
margins and then also resolved `justify-content` against the same un-zeroed free space, so any
value other than `flex-start` moved every item by the absorbed space a second time — items past
the container's edge, and a page with a phantom horizontal scrollbar behind them. The
`an_end_justified_row_keeps_its_items_inside` fixture in `crates/zgui-ui/tests/cycle_scroll.rs`
holds the workspace to the fixed behaviour.

### The scaled flex shrink factor, in intrinsic main sizes

One local fix in `src/compute/flexbox.rs` (`scaled_shrink_factor`, marked `zgui local patch`):

[§9.9.3](https://www.w3.org/TR/css-flexbox-1/#intrinsic-main-sizes) sizes a flex container to its
content by dividing each item's shortfall — its content contribution less its flex base size — by a
scaled shrink factor, taking the largest fraction on the line, and multiplying it back out. Upstream
divided by `max(1, shrink × basis)` and multiplied by `max(1, shrink) × basis`; the two agree only
once `flex-shrink` is at least one, and the spec's wording ("divide by its scaled flex shrink factor
having floored the flex shrink factor at 1") is the multiplier. An item with `flex-shrink: 0` and a
negative margin — an overlapping stack of avatars, a button pulling its mark out of its own padding
— therefore had its overlap come back multiplied by its whole width, and the container measured at a
fraction of its content or at nothing. Both halves now name one function, floored at one pixel so a
sub-pixel base size cannot divide by zero. The `an_overlapping_stack_measures_its_whole_width`
fixture in `crates/zgui-ui/tests/cycle_scroll.rs` holds the workspace to it.

The copy is consumed as a **path dependency** in the workspace `Cargo.toml`, not through
`[patch.crates-io]`: a patch section only takes effect in the root manifest of the workspace being
built, so any project depending on zgui by path would silently compile against the unpatched
release — and, since `is_fixed_position` extends a trait `zgui-layout` implements, fail to compile
at all. A path dependency travels with the crate graph wherever it is built from. Drop this copy
the release after both changes land upstream.
