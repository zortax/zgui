//! What a button looks like, in tokens.

use zgui::style;

style! { pub ButtonStyle =>
// The ring and the lift are two custom properties composed into one `box-shadow` rather than
// one declaration each state overwrites. An outlined button carries a shadow at rest and grows
// a focus ring on top of it; a single `box-shadow` would make those two mutually exclusive,
// and whichever rule cascaded last would silently delete the other.
":scope {
        display: inline-flex;
        flex-direction: row;
        flex-shrink: 0;
        align-items: center;
        justify-content: center;
        gap: var(--zui-space-sm);
        height: 34px;
        padding: 0 var(--zui-space-lg);
        border: 1px solid transparent;
        border-radius: var(--zui-radius-md);
        background-color: transparent;
        color: var(--zui-color-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-medium);
        line-height: var(--zui-type-leading-sm);
        white-space: nowrap;
        --zui-button-ring: 0 0 transparent;
        --zui-button-lift: 0 0 transparent;
        box-shadow: var(--zui-button-ring), var(--zui-button-lift);
        transition-property: background-color, border-color, color, box-shadow;
        transition-duration: var(--zui-motion-duration-normal);
        transition-timing-function: var(--zui-motion-ease-standard);
    }"

// A drawing inside a button is not a target of its own: a press anywhere on the button is the
// button's, including the part of it the mark covers.
":scope .zui-icon { pointer-events: none; flex: none; }"

// Every size is a height, a gap, a padding — and a second padding for when the button holds a
// mark rather than a word. A mark is squarer than a word, so a button that kept its full side
// padding around one reads as a button with a hole in it.
":scope[data-size=\"xs\"] {
        height: 24px;
        gap: var(--zui-space-xs);
        padding: 0 var(--zui-space-sm);
        font-size: var(--zui-type-size-xs);
        line-height: var(--zui-type-leading-xs);
        --zui-icon-md: 12px;
    }"
":scope[data-size=\"sm\"] {
        height: 30px;
        gap: calc(var(--zui-space-base) * 1.5);
        padding: 0 var(--zui-space-md);
    }"
":scope[data-size=\"lg\"] { height: 38px; padding: 0 var(--zui-space-xl); }"
":scope[data-size=\"icon\"] { width: 34px; height: 34px; padding: 0; }"
":scope[data-size=\"icon-xs\"] {
        width: 24px;
        height: 24px;
        padding: 0;
        --zui-icon-md: 12px;
    }"
":scope[data-size=\"icon-sm\"] { width: 30px; height: 30px; padding: 0; }"
":scope[data-size=\"icon-lg\"] { width: 38px; height: 38px; padding: 0; }"

// # A mark at one end sits closer to the edge than a word does
//
// A word needs its full side padding; a mark, being squarer and lighter, looks marooned at the
// same distance and wants a step less. The difference is one step of the space ladder at every
// size but the largest, where it is two.
//
// Said as a negative margin on the mark rather than as a smaller padding on the button, and
// that is not a preference. Narrowing the button's own padding means the button asking whether
// any of its children is a mark, which is a relative selector and this engine has none — the
// parity register's `:has()` row records it, and a rule written that way is not merely ignored
// but *dropped whole*, taking its declarations with it. Pulling the mark outward instead is an
// ordinary child selector, and lands it in the same place.
//
// It applies only at the sizes that hold a word beside the mark. The `icon` sizes are square
// and have no padding to pull out of.
":scope[data-size=\"md\"] > .zui-icon:first-child,
     :scope[data-size=\"md\"] > .zui-spinner:first-child {
        margin-inline-start: calc(var(--zui-space-base) * -1);
    }"
":scope[data-size=\"md\"] > .zui-icon:last-child,
     :scope[data-size=\"md\"] > .zui-spinner:last-child {
        margin-inline-end: calc(var(--zui-space-base) * -1);
    }"
":scope[data-size=\"xs\"] > .zui-icon:first-child,
     :scope[data-size=\"xs\"] > .zui-spinner:first-child,
     :scope[data-size=\"sm\"] > .zui-icon:first-child,
     :scope[data-size=\"sm\"] > .zui-spinner:first-child {
        margin-inline-start: calc(var(--zui-space-base) * -0.5);
    }"
":scope[data-size=\"xs\"] > .zui-icon:last-child,
     :scope[data-size=\"xs\"] > .zui-spinner:last-child,
     :scope[data-size=\"sm\"] > .zui-icon:last-child,
     :scope[data-size=\"sm\"] > .zui-spinner:last-child {
        margin-inline-end: calc(var(--zui-space-base) * -0.5);
    }"
":scope[data-size=\"lg\"] > .zui-icon:first-child,
     :scope[data-size=\"lg\"] > .zui-spinner:first-child {
        margin-inline-start: calc(var(--zui-space-base) * -2);
    }"
":scope[data-size=\"lg\"] > .zui-icon:last-child,
     :scope[data-size=\"lg\"] > .zui-spinner:last-child {
        margin-inline-end: calc(var(--zui-space-base) * -2);
    }"

":scope[data-variant=\"default\"] {
        background-color: var(--zui-color-primary);
        color: var(--zui-color-primary-foreground);
    }"
":scope[data-variant=\"default\"]:hover {
        background-color: color-mix(in oklab, var(--zui-color-primary) 90%, transparent);
    }"

// White rather than the destructive foreground token: a destructive button's label is plain
// white in either scheme, and that token carries the pale red a destructive *message* is set
// in, which on this fill is a tint rather than a legible label.
":scope[data-variant=\"destructive\"] {
        background-color: var(--zui-color-control-destructive-fill);
        color: #ffffff;
    }"
":scope[data-variant=\"destructive\"]:hover {
        background-color: color-mix(in oklab, var(--zui-color-destructive) 90%, transparent);
    }"
":scope[data-variant=\"destructive\"]:focus-visible {
        --zui-button-ring: 0 0 0 3px
            var(--zui-color-control-ring-invalid);
    }"

":scope[data-variant=\"outline\"] {
        border-color: var(--zui-color-control-outline-border);
        background-color: var(--zui-color-control-outline-fill);
        --zui-button-lift: var(--zui-shadow-xs);
    }"
":scope[data-variant=\"outline\"]:hover {
        background-color: var(--zui-color-control-outline-hover);
        color: var(--zui-color-accent-foreground);
    }"

":scope[data-variant=\"secondary\"] {
        background-color: var(--zui-color-secondary);
        color: var(--zui-color-secondary-foreground);
    }"
":scope[data-variant=\"secondary\"]:hover {
        background-color: color-mix(in oklab, var(--zui-color-secondary) 80%, transparent);
    }"

":scope[data-variant=\"ghost\"]:hover {
        background-color: var(--zui-color-control-ghost-hover);
        color: var(--zui-color-accent-foreground);
    }"

":scope[data-variant=\"link\"] {
        color: var(--zui-color-primary);
        text-underline-offset: 4px;
    }"
":scope[data-variant=\"link\"]:hover { text-decoration-line: underline; }"

// Every interaction state is CSS. There is no signal called `hovered` anywhere in this crate,
// and there is no listener that sets one: the engine already knows, and a component that
// tracked it would be a second answer that disagrees on the frame the pointer leaves.
//
// The ring is drawn outside the border and the border takes the ring's own colour, so the two
// read as one halo three pixels deep rather than as an outline standing off a grey edge.
":scope:focus-visible {
        border-color: var(--zui-color-ring);
        --zui-button-ring: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
":scope:invalid {
        border-color: var(--zui-color-destructive);
        --zui-button-ring: 0 0 0 3px
            var(--zui-color-control-ring-invalid);
    }"
":scope:disabled { opacity: 0.5; pointer-events: none; }"

// The dark scheme softens the two fills that would otherwise glare and gives the outlined
// button a filled body instead of a bare one. It is behind the desktop's own setting, which is
// the answer whenever the theme is left to follow it.
}
