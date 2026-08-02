//! What a resizable group looks like, in tokens.

use zgui::style;

style! { pub ResizableStyle =>
    ":scope { display: flex; flex-direction: row; width: 100%; height: 100%; }"
    ":scope[data-direction=\"vertical\"] { flex-direction: column; }"
    // The share is the basis and nothing grows: a panel that grew would take the room the user
    // gave to the panel beside it the first time the window changed size.
    ".zui-resizable__panel {
        display: flex;
        flex-direction: column;
        flex: 0 0 var(--zui-panel-size, auto);
        overflow: hidden;
    }"

    // One pixel to the eye and nine to the pointer. The element is the nine: an invisible strip
    // wide enough to actually catch a hand, taking one pixel of the row's room and leaning four
    // over each neighbour through its negative margins. The pixel someone sees is the line inside
    // it, a child box rather than generated content, because a hit has to resolve to *this*
    // element and a drawn child settles that without asking anything of pseudo-element hit paths.
    // The lean is what the z-index is for: the panel after the handle is painted later, and
    // without it the handle's trailing four pixels would be under that panel to the pointer too.
    ".zui-resizable__handle {
        position: relative;
        z-index: 10;
        display: flex;
        flex: 0 0 auto;
        align-items: center;
        justify-content: center;
        width: 9px;
        margin-left: -4px;
        margin-right: -4px;
        align-self: stretch;
        border: none;
        padding: 0;
        background-color: transparent;
    }"
    ".zui-resizable__line {
        position: absolute;
        top: 0;
        bottom: 0;
        left: 4px;
        width: 1px;
        background-color: var(--zui-color-border);
    }"
    ":scope[data-direction=\"vertical\"] > .zui-resizable__handle {
        width: auto;
        height: 9px;
        margin: -4px 0;
    }"
    ":scope[data-direction=\"vertical\"] > .zui-resizable__handle > .zui-resizable__line {
        top: 4px;
        bottom: auto;
        left: 0;
        right: 0;
        width: auto;
        height: 1px;
    }"
    // The ring wraps the line rather than the strip: the strip is nine invisible pixels, and a
    // ring around it would say the divider is somewhere it is not.
    ".zui-resizable__handle:focus-visible { outline: none; }"
    ".zui-resizable__handle:focus-visible > .zui-resizable__line {
        outline: 1px solid var(--zui-color-ring);
        outline-offset: 1px;
    }"

    // The optional grip: a tablet straddling the rule, with three dots on it. Turned a quarter in
    // a vertical group so the dots run along the divider rather than across it.
    ".zui-resizable__grip {
        --zui-icon-md: calc(var(--zui-space-base) * 2.5);
        position: relative;
        z-index: 1;
        display: flex;
        align-items: center;
        justify-content: center;
        width: var(--zui-space-md);
        height: var(--zui-space-lg);
        border: 1px solid var(--zui-color-border);
        border-radius: calc(var(--zui-radius-base) * 0.2);
        background-color: var(--zui-color-border);
        color: var(--zui-color-foreground);
    }"
    ".zui-resizable__grip .zui-icon { transform: rotate(90deg); }"
    ":scope[data-direction=\"vertical\"] .zui-resizable__grip { transform: rotate(90deg); }"
}
