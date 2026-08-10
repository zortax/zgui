# CSS parity

Generated. Every number here is measured by the conformance harness and none of it is
written by hand; regenerating it is part of the test suite, so a change to what the
framework supports arrives as a diff to this file.

A property counts as **implemented** when some module declares that it reads the value,
and that declaration is only believed when setting the property on a fixture visibly
changes one of four things: the fragment tree, the answer hit testing gives, the shapes
boxes are clipped to, or what a style lowers to for painting. A declaration with no such
consequence fails the build unless it is listed, with a reason, under *claimed without
observable consequence* below.

Parity here means parity with **what the style engine and the vector stack actually
support**. A feature that would need a patched or vendored build of the engine is out of
scope by decision — there is to be no fork — and is recorded under *out of reach* with
what an application should write instead. Everything else that is missing is work, and is
recorded separately.

## The numbers

Three answers, kept apart: what is **implemented**, what is **not yet implemented** — reachable
with the engine as it stands and simply not done — and what is **out of reach**, meaning no
build of this framework can do it without a patched style engine. The last is a boundary and
not a backlog, so it is never added to what is left to do.

| | Count |
|---|---:|
| Property names the engine generates | 322 |
| Distinct longhands behind them | 250 |
| Classified | 250 |
| Implemented | 178 |
| Parsed and cascaded, not yet implemented | 72 |
| Classified as unavailable from the engine | 0 |
| Shown by probe to change what a frame produces | 159 |
| Out of reach: defined by the engine for another target only | 172 |
| Out of reach: register rows | 7 |
| Not yet implemented: register rows | 0 |

## Converted reference suites

| Suite | Passing | Tests | Unconvertible |
|---|---:|---:|---:|
| `block-flow` | 2 | 2 | 0 |
| `css-sizing` | 2 | 2 | 0 |
| `flexbox` | 2 | 2 | 0 |
| `float` | 1 | 1 | 0 |
| `grid` | 2 | 2 | 0 |

Each test is compared against its reference as a fragment tree rather than as pixels,
and the counts above are a floor that may never fall.

## Declared twice, differently

Declarations live beside the code that reads a property, so two crates can each answer for
their own reasons. The stronger answer is the one counted, and both are shown.

| Property | Counted | Also declared |
|---|---|---|
| `clip-path` | implemented | unread |
| `font-language-override` | implemented | unread |

## Claimed without observable consequence

These properties reach a consumer that this harness cannot exercise. The deterministic
shaper has one face and applies no feature by design, because a suite written against real
faces measures the machine it runs on.

| Property | Why no probe settles it |
|---|---|
| `font-family` | the deterministic shaper has one face and never selects another |
| `font-weight` | the deterministic shaper synthesises no weight |
| `font-style` | the deterministic shaper synthesises no slant |
| `font-stretch` | the deterministic shaper has no width axis |
| `font-variation-settings` | the deterministic shaper instances no axis |
| `font-optical-sizing` | the deterministic shaper has no optical size axis |
| `font-feature-settings` | the deterministic shaper applies no OpenType feature |
| `font-kerning` | the deterministic shaper has no kerning to switch off |
| `font-variant-ligatures` | the deterministic shaper forms no ligature |
| `font-variant-caps` | the deterministic shaper has no small-capital coverage |
| `font-variant-position` | the deterministic shaper has no superior or inferior forms |
| `font-variant-numeric` | the deterministic shaper has one set of figures |
| `font-variant-east-asian` | the deterministic shaper has no East Asian variants |
| `font-language-override` | the deterministic shaper resolves no language system |
| `word-break` | the deterministic shaper breaks only at spaces |
| `overflow-wrap` | the deterministic shaper breaks only at spaces |
| `color` | a run's colour is a brush slot in the scene's paint table, not a fragment field |
| `cursor` | the harness has no window, so no pointer is over anything and no cursor is shown |
| `caret-color` | the harness has no window, so nothing has focus and no caret is drawn |
| `user-select` | the harness has no window, so nothing is pressed on and no selection is begun |

## Out of reach

Parity is parity with what the style engine and the vector stack support. These rows would
need a patched or vendored build of the engine, and there is to be none — so they are the
accepted boundary of this framework rather than work that is outstanding. Each says what
an application should write instead, and carries a probe, so a row that has quietly become
untrue fails.

| Missing | Why the stack cannot reach it | What to do instead | Standing |
|---|---|---|---|
| `:has()` | the servo selector parser answers `false` for relative selectors outright, with no preference behind it, so the rule is reported as an unexpected identifier and dropped whole — every declaration inside it with it | put the condition where the view already knows it: a component that renders a child conditionally knows it is doing so, so give the parent a class in the same expression — `class:has-icon=move || icon.is_some()` — and write the rule against that class. A parent that has to react to something further down hands a signal down and the child sets it | out of reach |
| `:nth-child(An+B of S)` | the selector-list form is hardcoded off in the same place and in the same way as `:has()`, so the `of` keyword is an unexpected token and the rule is dropped whole | count in the view rather than in the sheet: a list that renders its own items knows each one's index, so it can set a class on the ones a rule is meant to reach. `:nth-child()` without `of` is available and covers striping and every other position-only rule | out of reach |
| `::first-line` | two independent halves. The parser has no such pseudo-element, so the rule is dropped; and a first line's identity is known only after breaking, while its style changes shaping, so honouring it is a re-shape-after-break fixpoint rather than a restyle | a lead-in that is written as its own element — a `<text class="lede">` holding the first sentence — is styled by an ordinary class and needs no pseudo-element. A drop capital is the same shape of answer: one element, floated, sized | out of reach, and work here after that |
| `SVG paint: the whole inherited-SVG property group` | all twenty-one longhands are present in the engine's sources but generated only for another engine, so the group is not an active one in this build and every declaration using one is dropped at parse time | say it on the drawing rather than in the sheet: `fill`, `stroke` and the rest are read from the vector document's own attributes, so an icon that has to take a colour from its surroundings takes it as a property of the view that renders it — which is what the icon set ships and what `--zgui-*` custom properties feed | out of reach |
| `text-decoration-thickness, text-underline-offset, text-underline-position` | the line, its style and its colour are all generated and read; the three                  properties that say how thick it is and where it sits are generated only for                  another engine, so their names are unknown to the parser and a declaration using                  one is dropped whole | `text-decoration-line`, `-style` and `-color` are all read, and the line is drawn against the face's own metrics — which is where a browser starts from too. An underline that has to sit somewhere else is a border or a box of its own under the run | out of reach |
| `prefers-reduced-motion` | the engine's servo build carries a fixed list of media features and this is not on it, so the feature name is an error rather than a query that does not match — and the `@media` rule it heads is dropped whole, with every rule inside it | make it a fact about the document rather than about the device. An application that has discovered the preference sets an attribute on its root element and writes `[reduced-motion] .toast { animation: none }`, which is an ordinary attribute selector and works today. The tokens already ship a zero duration for exactly this, so a component built on them needs one rule and not one per animation | out of reach, and work here after that |
| `scrollbar-gutter` | stopping the window scrolling behind a modal surface means `overflow: hidden` on                  the root, which takes the scrollbar away and gives its gutter back to the                  content — so the page re-wraps and jumps sideways on the frame a dialog opens.                  The property that reserves the gutter is generated only for another engine, so                  its name is unknown to the parser and a declaration using it is dropped whole;                  layout's own scroll lock, which keeps the gutter a container was already                  reserving, is reachable from nothing above layout | keep the gutter yourself while a surface is up: `padding-right` on the scrolling element of the width the scrollbar reserves, applied by the same class that locks the scroll. A page whose scroll region is inside a fixed-size frame — which is what a desktop window usually is — never gives the gutter back and needs nothing | out of reach, and work here after that |

## Not yet implemented

Reachable with the engine exactly as it stands, and therefore work rather than boundary.

| Missing | Why it is missing | What would close it | Owner |
|---|---|---|---|
| — | — | — | — |

What it would take to close a row that is out of reach is recorded too, in
`zgui-css::parity::Gap::patch`, so that a future engine release can be measured against
it. It is not a plan.

## Every longhand

| Property | Treatment | Evidence | Where |
|---|---|---|---|
| `-webkit-text-security` | unread | no observable change | no probe has shown it changing a shaped line, and no module reads it |
| `align-content` | implemented | changes what a frame produces | zgui-layout::style::flex |
| `align-items` | implemented | changes what a frame produces | zgui-layout::style::flex |
| `align-self` | implemented | changes what a frame produces | zgui-layout::style::flex |
| `alignment-baseline` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `animation-composition` | unread | no observable change | the engine's animation driver does not read it: scroll-driven animations are parsed and never sampled |
| `animation-delay` | implemented | changes what a frame produces | zgui-style::driver::animations |
| `animation-direction` | implemented | changes what a frame produces | zgui-style::driver::animations |
| `animation-duration` | implemented | changes what a frame produces | zgui-style::driver::animations |
| `animation-fill-mode` | implemented | changes what a frame produces | zgui-style::driver::animations |
| `animation-iteration-count` | implemented | changes what a frame produces | zgui-style::driver::animations |
| `animation-name` | implemented | changes what a frame produces | zgui-style::driver::animations |
| `animation-play-state` | implemented | changes what a frame produces | zgui-style::driver::animations |
| `animation-range-end` | unread | no observable change | the engine's animation driver does not read it: scroll-driven animations are parsed and never sampled |
| `animation-range-start` | unread | no observable change | the engine's animation driver does not read it: scroll-driven animations are parsed and never sampled |
| `animation-timeline` | unread | no observable change | the engine's animation driver does not read it: scroll-driven animations are parsed and never sampled |
| `animation-timing-function` | implemented | changes what a frame produces | zgui-style::driver::animations |
| `aspect-ratio` | implemented | changes what a frame produces | zgui-layout::style::core |
| `backdrop-filter` | implemented | changes what a frame produces | zgui-layout::fragment |
| `backface-visibility` | implemented | changes what a frame produces | zgui-paint::lower::transform |
| `background-attachment` | unread | no observable change | there is no separately scrolling background layer |
| `background-blend-mode` | unread | no observable change | background layers do not blend with each other |
| `background-clip` | unread | no observable change | a background is painted to the border box |
| `background-color` | implemented | changes what a frame produces | zgui-paint::lower::background |
| `background-image` | implemented | changes what a frame produces | zgui-paint::lower::background |
| `background-origin` | unread | no observable change | a background layer fills the box it is painted on |
| `background-position-x` | unread | no observable change | a background layer fills the box it is painted on |
| `background-position-y` | unread | no observable change | a background layer fills the box it is painted on |
| `background-repeat` | unread | no observable change | a background layer fills the box it is painted on |
| `background-size` | unread | no observable change | a background layer fills the box it is painted on |
| `baseline-shift` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `baseline-source` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `block-size` | implemented | changes what a frame produces | zgui-layout::style::core |
| `border-block-end-color` | implemented | changes what a frame produces | zgui-paint::lower::border |
| `border-block-end-style` | implemented | changes what a frame produces | zgui-layout::style::core |
| `border-block-end-width` | implemented | changes what a frame produces | zgui-layout::style::core |
| `border-block-start-color` | implemented | changes what a frame produces | zgui-paint::lower::border |
| `border-block-start-style` | implemented | changes what a frame produces | zgui-layout::style::core |
| `border-block-start-width` | implemented | changes what a frame produces | zgui-layout::style::core |
| `border-bottom-color` | implemented | changes what a frame produces | zgui-paint::lower::border |
| `border-bottom-left-radius` | implemented | changes what a frame produces | zgui-paint::lower::border |
| `border-bottom-right-radius` | implemented | changes what a frame produces | zgui-paint::lower::border |
| `border-bottom-style` | implemented | changes what a frame produces | zgui-layout::style::core |
| `border-bottom-width` | implemented | changes what a frame produces | zgui-layout::style::core |
| `border-collapse` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `border-end-end-radius` | implemented | changes what a frame produces | zgui-layout::fragment |
| `border-end-start-radius` | implemented | changes what a frame produces | zgui-layout::fragment |
| `border-image-outset` | unread | no observable change | border images are not painted |
| `border-image-repeat` | unread | no observable change | border images are not painted |
| `border-image-slice` | unread | no observable change | border images are not painted |
| `border-image-source` | unread | no observable change | border images are not painted |
| `border-image-width` | unread | no observable change | border images are not painted |
| `border-inline-end-color` | implemented | changes what a frame produces | zgui-paint::lower::border |
| `border-inline-end-style` | implemented | changes what a frame produces | zgui-layout::style::core |
| `border-inline-end-width` | implemented | changes what a frame produces | zgui-layout::style::core |
| `border-inline-start-color` | implemented | changes what a frame produces | zgui-paint::lower::border |
| `border-inline-start-style` | implemented | changes what a frame produces | zgui-layout::style::core |
| `border-inline-start-width` | implemented | changes what a frame produces | zgui-layout::style::core |
| `border-left-color` | implemented | changes what a frame produces | zgui-paint::lower::border |
| `border-left-style` | implemented | changes what a frame produces | zgui-layout::style::core |
| `border-left-width` | implemented | changes what a frame produces | zgui-layout::style::core |
| `border-right-color` | implemented | changes what a frame produces | zgui-paint::lower::border |
| `border-right-style` | implemented | changes what a frame produces | zgui-layout::style::core |
| `border-right-width` | implemented | changes what a frame produces | zgui-layout::style::core |
| `border-spacing` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `border-start-end-radius` | implemented | changes what a frame produces | zgui-layout::fragment |
| `border-start-start-radius` | implemented | changes what a frame produces | zgui-layout::fragment |
| `border-top-color` | implemented | changes what a frame produces | zgui-paint::lower::border |
| `border-top-left-radius` | implemented | changes what a frame produces | zgui-layout::fragment |
| `border-top-right-radius` | implemented | changes what a frame produces | zgui-paint::lower::border |
| `border-top-style` | implemented | changes what a frame produces | zgui-layout::style::core |
| `border-top-width` | implemented | changes what a frame produces | zgui-layout::style::core |
| `bottom` | implemented | changes what a frame produces | zgui-layout::style::core |
| `box-shadow` | implemented | changes what a frame produces | zgui-layout::fragment |
| `box-sizing` | implemented | changes what a frame produces | zgui-layout::style::core |
| `caption-side` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `caret-color` | implemented | no observable change | zgui-runtime::window::caret |
| `clear` | implemented | changes what a frame produces | zgui-layout::style::core |
| `clip` | unread | no observable change | nothing paints yet, so nothing reads it |
| `clip-path` | implemented | changes what a frame produces | zgui-layout::fragment |
| `color` | implemented | changes what a frame produces | zgui-text-style::lower::paint |
| `color-scheme` | unread | no observable change | no probe has shown it changing a shaped line, and no module reads it |
| `column-count` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `column-gap` | implemented | changes what a frame produces | zgui-layout::style::flex |
| `column-span` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `column-width` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `contain` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `container-name` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `container-type` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `content` | implemented | changes what a frame produces | zgui-layout::style::core |
| `corner-bottom-left-shape` | unread | no observable change | nothing paints yet, so nothing reads it |
| `corner-bottom-right-shape` | unread | no observable change | nothing paints yet, so nothing reads it |
| `corner-end-end-shape` | unread | no observable change | nothing paints yet, so nothing reads it |
| `corner-end-start-shape` | unread | no observable change | nothing paints yet, so nothing reads it |
| `corner-start-end-shape` | unread | no observable change | nothing paints yet, so nothing reads it |
| `corner-start-start-shape` | unread | no observable change | nothing paints yet, so nothing reads it |
| `corner-top-left-shape` | unread | no observable change | nothing paints yet, so nothing reads it |
| `corner-top-right-shape` | unread | no observable change | nothing paints yet, so nothing reads it |
| `counter-increment` | unread | no observable change | nothing paints yet, so nothing reads it |
| `counter-reset` | unread | no observable change | nothing paints yet, so nothing reads it |
| `cursor` | implemented | no observable change | zgui-runtime::window::cursor |
| `direction` | implemented | changes what a frame produces | zgui-style::damage::a11y_key |
| `display` | implemented | changes what a frame produces | zgui-layout::style::core |
| `empty-cells` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `filter` | implemented | changes what a frame produces | zgui-layout::fragment |
| `flex-basis` | implemented | changes what a frame produces | zgui-layout::style::flex |
| `flex-direction` | implemented | changes what a frame produces | zgui-layout::style::flex |
| `flex-grow` | implemented | changes what a frame produces | zgui-layout::style::flex |
| `flex-shrink` | implemented | changes what a frame produces | zgui-layout::style::flex |
| `flex-wrap` | implemented | changes what a frame produces | zgui-layout::style::flex |
| `float` | implemented | changes what a frame produces | zgui-layout::style::core |
| `font-family` | implemented | no observable change | zgui-style::device::metrics |
| `font-feature-settings` | implemented | no observable change | zgui-text-parley::shape::style |
| `font-kerning` | implemented | no observable change | zgui-text-parley::shape::style |
| `font-language-override` | implemented | no observable change | zgui-style::device::metrics |
| `font-optical-sizing` | implemented | no observable change | zgui-text-parley::shape::style |
| `font-size` | implemented | changes what a frame produces | zgui-style::device::metrics |
| `font-stretch` | implemented | no observable change | zgui-style::device::metrics |
| `font-style` | implemented | no observable change | zgui-style::device::metrics |
| `font-synthesis-weight` | unread | no observable change | in the shaping key; the shaper offers no control over whether a weight is faked |
| `font-variant-caps` | implemented | no observable change | zgui-text-parley::shape::style |
| `font-variant-east-asian` | implemented | no observable change | zgui-text-parley::shape::style |
| `font-variant-ligatures` | implemented | no observable change | zgui-text-parley::shape::style |
| `font-variant-numeric` | implemented | no observable change | zgui-text-parley::shape::style |
| `font-variant-position` | implemented | no observable change | zgui-text-parley::shape::style |
| `font-variation-settings` | implemented | no observable change | zgui-style::device::metrics |
| `font-weight` | implemented | no observable change | zgui-style::device::metrics |
| `grid-auto-columns` | implemented | changes what a frame produces | zgui-layout::style::grid |
| `grid-auto-flow` | implemented | changes what a frame produces | zgui-layout::style::grid |
| `grid-auto-rows` | implemented | changes what a frame produces | zgui-layout::style::grid |
| `grid-column-end` | implemented | changes what a frame produces | zgui-layout::style::grid |
| `grid-column-start` | implemented | changes what a frame produces | zgui-layout::style::grid |
| `grid-row-end` | implemented | changes what a frame produces | zgui-layout::style::grid |
| `grid-row-start` | implemented | changes what a frame produces | zgui-layout::style::grid |
| `grid-template-areas` | implemented | changes what a frame produces | zgui-layout::style::grid |
| `grid-template-columns` | implemented | changes what a frame produces | zgui-layout::style::grid |
| `grid-template-rows` | implemented | changes what a frame produces | zgui-layout::style::grid |
| `height` | implemented | changes what a frame produces | zgui-layout::style::core |
| `image-rendering` | unread | no observable change | no probe has shown it changing a shaped line, and no module reads it |
| `inline-size` | implemented | changes what a frame produces | zgui-layout::style::core |
| `inset-block-end` | implemented | changes what a frame produces | zgui-layout::style::core |
| `inset-block-start` | implemented | changes what a frame produces | zgui-layout::style::core |
| `inset-inline-end` | implemented | changes what a frame produces | zgui-layout::style::core |
| `inset-inline-start` | implemented | changes what a frame produces | zgui-layout::style::core |
| `isolation` | implemented | changes what a frame produces | zgui-layout::fragment |
| `justify-content` | implemented | changes what a frame produces | zgui-layout::style::flex |
| `justify-items` | implemented | changes what a frame produces | zgui-layout::style::flex |
| `justify-self` | implemented | changes what a frame produces | zgui-layout::style::flex |
| `left` | implemented | changes what a frame produces | zgui-layout::style::core |
| `letter-spacing` | implemented | changes what a frame produces | zgui-text-parley::shape::style |
| `line-break` | unread | no observable change | in the breaking key; the line breaker has no strictness control to hand it to |
| `line-height` | implemented | changes what a frame produces | zgui-style::engine::stylist |
| `list-style-image` | unread | no observable change | nothing paints yet, so nothing reads it |
| `list-style-position` | unread | no observable change | nothing paints yet, so nothing reads it |
| `list-style-type` | unread | no observable change | nothing paints yet, so nothing reads it |
| `margin-block-end` | implemented | changes what a frame produces | zgui-layout::style::core |
| `margin-block-start` | implemented | changes what a frame produces | zgui-layout::style::core |
| `margin-bottom` | implemented | changes what a frame produces | zgui-layout::style::core |
| `margin-inline-end` | implemented | changes what a frame produces | zgui-layout::style::core |
| `margin-inline-start` | implemented | changes what a frame produces | zgui-layout::style::core |
| `margin-left` | implemented | changes what a frame produces | zgui-layout::style::core |
| `margin-right` | implemented | changes what a frame produces | zgui-layout::style::core |
| `margin-top` | implemented | changes what a frame produces | zgui-layout::style::core |
| `mask-clip` | unread | no observable change | nothing paints yet, so nothing reads it |
| `mask-composite` | unread | no observable change | nothing paints yet, so nothing reads it |
| `mask-image` | unread | no observable change | nothing paints yet, so nothing reads it |
| `mask-mode` | unread | no observable change | nothing paints yet, so nothing reads it |
| `mask-origin` | unread | no observable change | nothing paints yet, so nothing reads it |
| `mask-position-x` | unread | no observable change | nothing paints yet, so nothing reads it |
| `mask-position-y` | unread | no observable change | nothing paints yet, so nothing reads it |
| `mask-repeat` | unread | no observable change | nothing paints yet, so nothing reads it |
| `mask-size` | unread | no observable change | nothing paints yet, so nothing reads it |
| `mask-type` | unread | no observable change | nothing paints yet, so nothing reads it |
| `max-block-size` | implemented | changes what a frame produces | zgui-layout::style::core |
| `max-height` | implemented | changes what a frame produces | zgui-layout::style::core |
| `max-inline-size` | implemented | changes what a frame produces | zgui-layout::style::core |
| `max-width` | implemented | changes what a frame produces | zgui-layout::style::core |
| `min-block-size` | implemented | changes what a frame produces | zgui-layout::style::core |
| `min-height` | implemented | changes what a frame produces | zgui-layout::style::core |
| `min-inline-size` | implemented | changes what a frame produces | zgui-layout::style::core |
| `min-width` | implemented | changes what a frame produces | zgui-layout::style::core |
| `mix-blend-mode` | implemented | changes what a frame produces | zgui-layout::fragment |
| `object-fit` | implemented | changes what a frame produces | zgui-paint::emit::replaced |
| `object-position` | implemented | changes what a frame produces | zgui-paint::emit::replaced |
| `offset-path` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `opacity` | implemented | changes what a frame produces | zgui-layout::fragment |
| `order` | implemented | changes what a frame produces | zgui-layout::style::flex |
| `outline-color` | implemented | changes what a frame produces | zgui-paint::lower::outline |
| `outline-offset` | implemented | changes what a frame produces | zgui-layout::fragment |
| `outline-style` | implemented | changes what a frame produces | zgui-layout::fragment |
| `outline-width` | implemented | changes what a frame produces | zgui-layout::fragment |
| `overflow-block` | implemented | changes what a frame produces | zgui-layout::style::core |
| `overflow-clip-margin` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `overflow-inline` | implemented | changes what a frame produces | zgui-layout::style::core |
| `overflow-wrap` | implemented | no observable change | zgui-text-parley::shape::style |
| `overflow-x` | implemented | changes what a frame produces | zgui-layout::style::core |
| `overflow-y` | implemented | changes what a frame produces | zgui-layout::style::core |
| `padding-block-end` | implemented | changes what a frame produces | zgui-layout::style::core |
| `padding-block-start` | implemented | changes what a frame produces | zgui-layout::style::core |
| `padding-bottom` | implemented | changes what a frame produces | zgui-layout::style::core |
| `padding-inline-end` | implemented | changes what a frame produces | zgui-layout::style::core |
| `padding-inline-start` | implemented | changes what a frame produces | zgui-layout::style::core |
| `padding-left` | implemented | changes what a frame produces | zgui-layout::style::core |
| `padding-right` | implemented | changes what a frame produces | zgui-layout::style::core |
| `padding-top` | implemented | changes what a frame produces | zgui-layout::style::core |
| `perspective` | implemented | changes what a frame produces | zgui-paint::lower::transform |
| `perspective-origin` | unread | no observable change | the perspective matrix is not composed yet |
| `pointer-events` | implemented | changes what a frame produces | zgui-layout::fragment |
| `position` | implemented | changes what a frame produces | zgui-layout::style::core |
| `position-area` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `position-try-fallbacks` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `quotes` | unread | no observable change | nothing paints yet, so nothing reads it |
| `right` | implemented | changes what a frame produces | zgui-layout::style::core |
| `rotate` | implemented | changes what a frame produces | zgui-layout::fragment |
| `row-gap` | implemented | changes what a frame produces | zgui-layout::style::flex |
| `scale` | implemented | changes what a frame produces | zgui-layout::fragment |
| `tab-size` | implemented | changes what a frame produces | zgui-layout::inline::content::generate |
| `table-layout` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `text-align` | implemented | changes what a frame produces | zgui-text-parley::shape::style |
| `text-align-last` | unread | no observable change | in the breaking key; the aligner has no separate treatment for the final line |
| `text-decoration-color` | implemented | changes what a frame produces | zgui-paint::emit::text |
| `text-decoration-line` | implemented | changes what a frame produces | zgui-paint::emit::text |
| `text-decoration-style` | implemented | changes what a frame produces | zgui-paint::emit::text |
| `text-indent` | implemented | changes what a frame produces | zgui-text-parley::shape::style |
| `text-justify` | unread | no observable change | in the breaking key; a justified line is stretched one way and the keyword picks none |
| `text-overflow` | implemented | changes what a frame produces | zgui-layout::inline::ellipsis |
| `text-rendering` | unread | no observable change | no probe has shown it changing a shaped line, and no module reads it |
| `text-shadow` | implemented | changes what a frame produces | zgui-paint::lower::shadow |
| `text-transform` | implemented | changes what a frame produces | zgui-layout::inline::content::generate |
| `text-wrap-mode` | implemented | changes what a frame produces | zgui-text-parley::shape::style |
| `top` | implemented | changes what a frame produces | zgui-layout::style::core |
| `transform` | implemented | changes what a frame produces | zgui-layout::fragment |
| `transform-origin` | implemented | changes what a frame produces | zgui-layout::fragment |
| `transform-style` | implemented | changes what a frame produces | zgui-paint::lower::transform |
| `transition-behavior` | implemented | changes what a frame produces | zgui-style::driver::animations |
| `transition-delay` | implemented | changes what a frame produces | zgui-style::driver::animations |
| `transition-duration` | implemented | changes what a frame produces | zgui-style::driver::animations |
| `transition-property` | implemented | changes what a frame produces | zgui-style::driver::animations |
| `transition-timing-function` | implemented | changes what a frame produces | zgui-style::driver::animations |
| `translate` | implemented | changes what a frame produces | zgui-layout::fragment |
| `unicode-bidi` | unread | no observable change | no probe has shown it changing a shaped line, and no module reads it |
| `user-select` | implemented | no observable change | zgui-runtime::window::select |
| `visibility` | implemented | changes what a frame produces | zgui-style::damage::a11y_key |
| `white-space-collapse` | implemented | changes what a frame produces | zgui-layout::inline::content::generate |
| `width` | implemented | changes what a frame produces | zgui-layout::style::core |
| `will-change` | unread | no observable change | no probe has shown it moving an edge, and no module reads it |
| `word-break` | implemented | no observable change | zgui-text-parley::shape::style |
| `word-spacing` | implemented | changes what a frame produces | zgui-text-parley::shape::style |
| `writing-mode` | unread | no observable change | in the shaping key; there is no vertical inline formatting context to lay text out in |
| `z-index` | implemented | changes what a frame produces | zgui-layout::fragment |
