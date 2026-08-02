//! What a group of toggles looks like, in tokens.
//!
//! Mostly the strip. Each item inside it is a [`Toggle`](crate::Toggle) and looks like one, so the
//! group has no rules of its own for the pressed state, the ring or the disabled treatment — only
//! for the seam, which is the one thing an item cannot know about on its own.

use zgui::style;

style! { pub ToggleGroupStyle =>
    // The gap is a multiple of the spacing step, written by the component from its `spacing` prop.
    // At zero the items meet, and the rules below turn that meeting into one seam.
    ":scope {
        display: inline-flex;
        flex-direction: row;
        width: fit-content;
        align-items: center;
        gap: calc(var(--zui-space-base) * var(--zui-toggle-group-gap, 0));
        border-radius: var(--zui-radius-md);
    }"
    ":scope[data-orientation=\"vertical\"] { flex-direction: column; }"

    // Every item is the same width whatever it holds, and takes the group's own side padding
    // rather than the padding its size would give it standing alone.
    ":scope[data-spacing] > .zui-toggle-group__item {
        width: auto;
        min-width: 0;
        flex-shrink: 0;
        padding: 0 var(--zui-space-md);
    }"
    // A focused item is lifted above its neighbours, because its ring is drawn outside its own box
    // and would otherwise be covered by whichever item comes after it.
    ":scope[data-spacing] > .zui-toggle-group__item:focus-visible { z-index: 10; }"

    // The seam. Nothing is rounded except the two ends, nothing carries a shadow of its own, and
    // an outlined item drops the border on the side facing its neighbour — so two items meet on
    // one line rather than two, and the strip reads as a single control.
    ":scope[data-spacing=\"0\"] > .zui-toggle-group__item {
        border-radius: 0;
        box-shadow: none;
    }"
    ":scope[data-spacing=\"0\"][data-orientation=\"horizontal\"]
        > .zui-toggle-group__item:first-child {
        border-top-left-radius: var(--zui-radius-md);
        border-bottom-left-radius: var(--zui-radius-md);
    }"
    ":scope[data-spacing=\"0\"][data-orientation=\"horizontal\"]
        > .zui-toggle-group__item:last-child {
        border-top-right-radius: var(--zui-radius-md);
        border-bottom-right-radius: var(--zui-radius-md);
    }"
    ":scope[data-spacing=\"0\"][data-orientation=\"horizontal\"]
        > .zui-toggle-group__item[data-variant=\"outline\"] {
        border-left-width: 0;
    }"
    ":scope[data-spacing=\"0\"][data-orientation=\"horizontal\"]
        > .zui-toggle-group__item[data-variant=\"outline\"]:first-child {
        border-left-width: 1px;
    }"
    ":scope[data-spacing=\"0\"][data-orientation=\"vertical\"]
        > .zui-toggle-group__item:first-child {
        border-top-left-radius: var(--zui-radius-md);
        border-top-right-radius: var(--zui-radius-md);
    }"
    ":scope[data-spacing=\"0\"][data-orientation=\"vertical\"]
        > .zui-toggle-group__item:last-child {
        border-bottom-left-radius: var(--zui-radius-md);
        border-bottom-right-radius: var(--zui-radius-md);
    }"
    ":scope[data-spacing=\"0\"][data-orientation=\"vertical\"]
        > .zui-toggle-group__item[data-variant=\"outline\"] {
        border-top-width: 0;
    }"
    ":scope[data-spacing=\"0\"][data-orientation=\"vertical\"]
        > .zui-toggle-group__item[data-variant=\"outline\"]:first-child {
        border-top-width: 1px;
    }"
    // The shadow the items gave up belongs to the strip instead, once rather than per item.
    ":scope[data-spacing=\"0\"][data-variant=\"outline\"] {
        box-shadow: var(--zui-shadow-xs);
    }"
}
