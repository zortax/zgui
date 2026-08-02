//! What a popover looks like, in tokens.

use zgui::style;

style! { pub PopoverStyle =>
    // The border, the radius, the background and the shadow are the shared surface's, and a
    // popover is the surface those were measured from. What is left is how wide it is and how far
    // its content sits from its edges.
    ":scope {
        width: 288px;
        padding: var(--zui-space-lg);
        outline: none;
    }"

    ".zui-popover__header {
        display: flex;
        flex-direction: column;
        gap: var(--zui-space-xs);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
    }"
    ".zui-popover__title { font-weight: var(--zui-type-weight-medium); }"
    ".zui-popover__description { color: var(--zui-color-muted-foreground); }"
}
