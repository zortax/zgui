//! What an alert looks like, in tokens.

use zgui::style;

style! { pub AlertStyle =>
    // Two columns: the mark, and everything that is written. A grid rather than a row of a mark
    // beside a stack, because the title and the description line up with *each other* down the
    // second column — a stack inside a row lines up with itself and drifts from the mark beside it
    // the moment either grows a line. With no mark the first column is nothing wide and the grid
    // reads as a stack.
    ":scope {
        position: relative;
        display: grid;
        grid-template-columns: 0 1fr;
        align-items: start;
        row-gap: calc(var(--zui-space-base) * 0.5);
        width: 100%;
        padding: var(--zui-space-md) var(--zui-space-lg);
        border: 1px solid var(--zui-color-border);
        border-radius: var(--zui-radius-lg);
        background-color: var(--zui-color-card);
        color: var(--zui-color-card-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
    }"
    ":scope[data-icon=\"true\"] {
        grid-template-columns: var(--zui-space-lg) 1fr;
        column-gap: var(--zui-space-md);
    }"

    // A destructive alert keeps the ordinary surface and the ordinary border and says what it is in
    // the text alone. A red box with a red edge and red writing shouts three times.
    ":scope[data-variant=\"destructive\"] { color: var(--zui-color-destructive); }"
    ":scope[data-variant=\"destructive\"] .zui-alert__description {
        color: color-mix(in oklab, var(--zui-color-destructive) 90%, transparent);
    }"

    // Nudged down half a step so its optical centre sits on the title's first line rather than on
    // that line's box, which is what makes a mark beside text look level instead of high.
    ".zui-alert__icon {
        grid-column-start: 1;
        grid-row-start: 1;
        transform: translateY(2px);
        color: currentColor;
        pointer-events: none;
    }"
    ".zui-alert__title {
        grid-column-start: 2;
        min-height: var(--zui-space-lg);
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
        font-weight: var(--zui-type-weight-medium);
        letter-spacing: var(--zui-type-tracking-tight);
    }"
    ".zui-alert__description {
        grid-column-start: 2;
        display: grid;
        justify-items: start;
        gap: var(--zui-space-xs);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        color: var(--zui-color-muted-foreground);
    }"
}
