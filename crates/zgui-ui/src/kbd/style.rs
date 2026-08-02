//! What a key and a run of keys look like, in tokens.

use zgui::style;

style! { pub KbdStyle =>
    ":scope {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: var(--zui-space-xs);
        height: 20px;
        min-width: 20px;
        width: fit-content;
        padding: 0 var(--zui-space-xs);
        border-radius: var(--zui-radius-sm);
        background-color: var(--zui-color-muted);
        color: var(--zui-color-muted-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-xs);
        font-weight: var(--zui-type-weight-medium);
        pointer-events: none;
        user-select: none;
    }"
    // A mark on a keycap is smaller than one in a line of text: the cap is twenty pixels tall and
    // a sixteen-pixel drawing inside it leaves no rim.
    ":scope .zui-icon { --zui-icon-md: 12px; }"
    // A tooltip is painted in the page's colours inverted, so a keycap inside one cannot be the
    // pale grey it is everywhere else — it would vanish. On that surface the cap is a wash of the
    // *page* colour, and its lettering is the page colour outright.
    ".zui-tooltip :scope {
        background-color: color-mix(in oklab, var(--zui-color-background) 20%, transparent);
        color: var(--zui-color-background);
    }"
    ".zui-kbd-group {
        display: inline-flex;
        align-items: center;
        gap: var(--zui-space-xs);
    }"
}
