//! What a breadcrumb trail looks like, in tokens.

use zgui::style;

style! { pub BreadcrumbStyle =>
    ":scope { display: block; }"
    ".zui-breadcrumb__list {
        display: flex;
        flex-direction: row;
        flex-wrap: wrap;
        align-items: center;
        gap: calc(var(--zui-space-base) * 1.5);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        color: var(--zui-color-muted-foreground);
        overflow-wrap: break-word;
    }"
    ".zui-breadcrumb__item {
        display: inline-flex;
        flex-direction: row;
        align-items: center;
        gap: calc(var(--zui-space-base) * 1.5);
    }"
    ".zui-breadcrumb__link {
        border: none;
        padding: 0;
        background-color: transparent;
        color: inherit;
        font: inherit;
        border-radius: var(--zui-radius-sm);
        transition: color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ".zui-breadcrumb__link:hover { color: var(--zui-color-foreground); }"
    ".zui-breadcrumb__link:focus-visible {
        outline: none;
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
    // The page you are on is the same weight as the crumbs behind it and only a different colour.
    // Bolding it would make the trail read as a heading with a path in front of it.
    ".zui-breadcrumb__page {
        color: var(--zui-color-foreground);
        font-weight: var(--zui-type-weight-normal);
    }"
    ".zui-breadcrumb__separator {
        display: inline-flex;
        align-items: center;
        color: var(--zui-color-muted-foreground);
    }"
    // Sized by moving the icon scale's own step rather than by overriding the length it resolves
    // to, so an application that has moved that scale moves this with it.
    ".zui-breadcrumb__separator { --zui-icon-md: calc(var(--zui-space-base) * 3.5); }"
    // A square the size of a control, so a trail whose middle has been folded away keeps the same
    // height as one that has not.
    ".zui-breadcrumb__ellipsis {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: calc(var(--zui-space-base) * 9);
        height: calc(var(--zui-space-base) * 9);
    }"
}
