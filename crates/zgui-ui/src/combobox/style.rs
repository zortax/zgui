//! What a combobox looks like, in tokens.
//!
//! The options are a [`Select`](crate::Select)'s: same padding, same rounding, same highlight. They
//! are the same thing seen twice — a list one row is taken from — and two sheets saying so would be
//! two places for the row height of a chooser to drift apart.

use zgui::style;

style! { pub ComboboxStyle =>
    ":scope { display: inline-flex; flex-direction: column; }"
    // Quiet, and no target of its own: the whole frame opens the list, so a mark that took a press
    // would be a second thing to hit inside one control.
    ".zui-combobox__mark { color: var(--zui-color-muted-foreground); pointer-events: none; }"
    // Tall enough to be worth scrolling and short enough to stay a list rather than a page. The
    // scroll is the list's own, so the field above it stays put while the options move.
    ".zui-combobox__list {
        padding: var(--zui-space-xs);
        min-width: 220px;
        max-height: 384px;
        overflow-y: auto;
    }"
    // Centred and quiet: it is the answer to a search rather than a row that can be chosen, and a
    // left-aligned line at option height would read as one.
    ".zui-combobox__empty {
        display: flex;
        width: 100%;
        justify-content: center;
        padding: var(--zui-space-sm) 0;
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        text-align: center;
        color: var(--zui-color-muted-foreground);
    }"
}
