//! What a pager looks like, in tokens.

use zgui::style;

style! { pub PaginationStyle =>
    ":scope {
        display: flex;
        flex-direction: row;
        width: 100%;
        margin-inline: auto;
        justify-content: center;
    }"
    ".zui-pagination__content {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: var(--zui-space-xs);
    }"
    ".zui-pagination__item { display: inline-flex; }"

    // A page number is a square the height of a control, and the two ends are ordinary controls
    // with a word in them. Both are the button's shape without being buttons: what they do is go
    // somewhere, and a reader is told so.
    ".zui-pagination__link {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: var(--zui-space-xs);
        width: calc(var(--zui-space-base) * 9);
        height: calc(var(--zui-space-base) * 9);
        padding: 0;
        border: 1px solid transparent;
        border-radius: var(--zui-radius-md);
        background-color: transparent;
        color: var(--zui-color-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-medium);
        line-height: var(--zui-type-leading-sm);
        white-space: nowrap;
        transition: background-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    box-shadow var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ".zui-pagination__previous, .zui-pagination__next {
        width: auto;
        padding-inline: calc(var(--zui-space-base) * 2.5);
    }"
    ".zui-pagination__link:hover {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"
    // The page being shown is outlined rather than filled: it is where you are, not what to press.
    ".zui-pagination__link[data-current=\"true\"] {
        border-color: var(--zui-color-border);
        background-color: var(--zui-color-background);
        box-shadow: var(--zui-shadow-xs);
    }"
    ".zui-pagination__link[data-current=\"true\"]:hover {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"
    ".zui-pagination__link:focus-visible {
        outline: none;
        border-color: var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
    ".zui-pagination__link:disabled { opacity: 0.5; pointer-events: none; }"
    ".zui-pagination__ellipsis {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: calc(var(--zui-space-base) * 9);
        height: calc(var(--zui-space-base) * 9);
        color: var(--zui-color-muted-foreground);
    }"
}
