//! What a scroll area looks like, in tokens.

use zgui::style;

style! { pub ScrollAreaStyle =>
    ":scope { position: relative; display: flex; overflow: hidden; }"
    // The rounding is the region's, inherited rather than restated: a scroll area with a rounded
    // border whose content ran square into the corners would show the content through them.
    ".zui-scroll-area__viewport {
        flex: 1 1 auto;
        overflow: auto;
        width: 100%;
        height: 100%;
        border-radius: inherit;
        transition: box-shadow var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ".zui-scroll-area__viewport:focus-visible {
        outline: 1px solid var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
    // Nothing here draws a track or a thumb. Both are composed by the engine into the gutter the
    // viewport reserved, directly beneath this element — so the bar takes no pointer events, since
    // a press has to reach the real bar underneath, and it is laid over exactly the strip that bar
    // occupies so that a focus ring frames the thing it operates rather than a rectangle beside it.
    //
    // It is always there, and whether it is *reachable* is `data-scrollable`, which is why nothing
    // here sets `display`: a control removed from the layout takes its tab stop and its
    // announcement with it.
    ".zui-scroll-area__bar {
        position: absolute;
        display: block;
        border: none;
        background-color: transparent;
        pointer-events: none;
    }"
    ".zui-scroll-area__bar[data-orientation=\"vertical\"] {
        top: 0;
        right: 0;
        bottom: 0;
        width: var(--zui-scrollbar-width, 15px);
    }"
    ".zui-scroll-area__bar[data-orientation=\"horizontal\"] {
        left: 0;
        right: 0;
        bottom: 0;
        height: var(--zui-scrollbar-width, 15px);
    }"
    ".zui-scroll-area__bar:focus-visible {
        outline: 1px solid var(--zui-color-ring);
        outline-offset: -1px;
        box-shadow: inset 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
}
