//! What a checkbox looks like, in tokens.

use zgui::style;

style! { pub CheckboxStyle =>
// A single-cell grid rather than a row, because the box holds *both* of its marks at once and
// shows one of them. `opacity: 0` hides a mark from the eye and not from layout, so a row lays
// the two of them out side by side and centres the pair — which puts whichever one is showing
// half its own width off to one side of the box. Stacked in one cell they are two answers in
// the same place, and the one that is showing is in the middle of the box whichever it is.
//
// The corner is four flat pixels rather than a step of the ladder: at sixteen pixels square the
// small step rounds the box until it stops reading as a box.
":scope {
        display: inline-grid;
        align-items: center;
        justify-items: center;
        width: 16px;
        height: 16px;
        flex: none;
        border: 1px solid var(--zui-color-input);
        border-radius: 4px;
        background-color: var(--zui-color-control-field);
        color: var(--zui-color-primary-foreground);
        box-shadow: var(--zui-shadow-xs);
        outline: none;
        transition-property: box-shadow, background-color, border-color;
        transition-duration: var(--zui-motion-duration-normal);
        transition-timing-function: var(--zui-motion-ease-standard);
    }"
// `:checked` and `:indeterminate` are engine states this component sets, not classes it
// computes: the appearance follows the same answer the accessibility tree is given.
":scope:checked, :scope:indeterminate {
        background-color: var(--zui-color-primary);
        border-color: var(--zui-color-primary);
    }"
":scope:focus-visible {
        border-color: var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent),
                    var(--zui-shadow-xs);
    }"
":scope:invalid { border-color: var(--zui-color-destructive); }"
":scope:invalid:focus-visible {
        border-color: var(--zui-color-destructive);
        box-shadow: 0 0 0 3px var(--zui-color-control-ring-invalid),
                    var(--zui-shadow-xs);
    }"
":scope:disabled { opacity: 0.5; pointer-events: none; }"
// The mark is drawn always and revealed by state, so ticking one does not build a subtree.
// Both marks name the same cell, which is what stacks them. It appears at once rather than
// fading in: a tick that arrives after the box has filled reads as a lag.
":scope > .zui-icon { grid-area: 1 / 1; opacity: 0; --zui-icon-size: 14px; }"
":scope:checked > .zui-checkbox__tick { opacity: 1; }"
":scope:indeterminate > .zui-checkbox__dash { opacity: 1; }"
// Faintly filled on a dark page, where an unticked box drawn in a hairline alone is a box
// nobody finds. The ticked fill is the same either way — it is the tint, and the tint is what
// *ticked* means.
}
