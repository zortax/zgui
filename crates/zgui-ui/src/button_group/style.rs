//! What a group of buttons looks like, in tokens.

use zgui::style;

style! { pub ButtonGroupStyle =>
    // The seam is made by the children rather than the group: every child but the first loses the
    // rounding and the border on the side facing its neighbour, so two buttons meet on one line
    // instead of two and the strip reads as a single control.
    ":scope { display: flex; width: fit-content; align-items: stretch; }"
    // A group of groups is not a seam. Nested strips are separate controls that happen to be in a
    // row, so they get a gap rather than a shared edge. Said as a margin on the nested strip rather
    // than as a gap on the one holding it, because reaching *up* from a child to the parent that
    // has one is a relative selector, and this engine has none — see the parity register's
    // `:has()` row. A margin on every nested strip but the first is the same distance.
    ":scope > .zui-button-group { margin-inline-start: var(--zui-space-sm); }"
    ":scope > .zui-button-group:first-child { margin-inline-start: 0; }"
    // The other half of the same distance: whatever follows a nested strip stands off it too, so a
    // plain control after a group is not glued to a strip it is no part of.
    ":scope > .zui-button-group + * { margin-inline-start: var(--zui-space-sm); }"
    // A focused member is lifted above its neighbours: its ring is drawn outside its own box, and
    // whichever member comes after it would otherwise paint over half of it.
    ":scope > *:focus-visible { position: relative; z-index: 10; }"

    ":scope[data-orientation=\"horizontal\"] > *:not(:first-child) {
        border-top-left-radius: 0;
        border-bottom-left-radius: 0;
        border-left-width: 0;
    }"
    ":scope[data-orientation=\"horizontal\"] > *:not(:last-child) {
        border-top-right-radius: 0;
        border-bottom-right-radius: 0;
    }"
    ":scope[data-orientation=\"vertical\"] { flex-direction: column; }"
    ":scope[data-orientation=\"vertical\"] > *:not(:first-child) {
        border-top-left-radius: 0;
        border-top-right-radius: 0;
        border-top-width: 0;
    }"
    ":scope[data-orientation=\"vertical\"] > *:not(:last-child) {
        border-bottom-left-radius: 0;
        border-bottom-right-radius: 0;
    }"

    ".zui-button-group__text {
        display: flex;
        align-items: center;
        gap: var(--zui-space-sm);
        padding: 0 var(--zui-space-lg);
        border: 1px solid var(--zui-color-border);
        border-radius: var(--zui-radius-md);
        background-color: var(--zui-color-muted);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-medium);
        line-height: var(--zui-type-leading-sm);
        box-shadow: var(--zui-shadow-xs);
    }"
    ".zui-button-group__text .zui-icon { pointer-events: none; }"

    // The line inside a strip is the field border rather than the page border: it divides two
    // parts of one control, and the darker of the two greys is what reads as a division.
    ".zui-button-group__separator {
        position: relative;
        flex-shrink: 0;
        margin: 0;
        background-color: var(--zui-color-input);
        align-self: stretch;
        width: 1px;
    }"
    ":scope[data-orientation=\"vertical\"] > .zui-button-group__separator {
        width: auto;
        height: 1px;
    }"
}
