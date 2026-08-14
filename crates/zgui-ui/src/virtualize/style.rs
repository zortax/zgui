//! What a virtualised list looks like, in tokens.

use zgui::style;

style! { pub VirtualListStyle =>
    ":scope {
        display: block;
        overflow-y: auto;
        overflow-x: hidden;
        position: relative;
    }"
    // The two spacers are the container's own padding rather than two extra elements: a box that
    // exists only to be empty is a box the style engine, layout and the painter each visit for
    // nothing, twice per frame, for ever.
    //
    // The component writes the two lengths straight onto the pane, and they are deliberately not
    // custom properties. A custom property inherits, so a lead that moves every time the window
    // does would change the inherited environment of every row inside it — and a row recascaded
    // for a scroll is the one thing a virtualised list exists to avoid. Nothing but the pane ever
    // read them.
    ":scope > .zui-virtual-list__pane {
        display: flex;
        flex-direction: column;
    }"
    ":scope > .zui-virtual-list__pane > * {
        height: var(--zui-virtual-row);
        flex: none;
        box-sizing: border-box;
    }"
}
