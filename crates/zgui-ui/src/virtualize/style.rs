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
    ":scope > .zui-virtual-list__pane {
        display: flex;
        flex-direction: column;
        padding-top: var(--zui-virtual-lead, 0px);
        padding-bottom: var(--zui-virtual-trail, 0px);
    }"
    ":scope > .zui-virtual-list__pane > * {
        height: var(--zui-virtual-row);
        flex: none;
        box-sizing: border-box;
    }"
}
