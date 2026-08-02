//! What a badge looks like, in tokens.

use zgui::style;

style! { pub BadgeStyle =>
":scope {
        display: inline-flex;
        flex-direction: row;
        flex-shrink: 0;
        align-items: center;
        justify-content: center;
        gap: var(--zui-space-xs);
        width: fit-content;
        overflow: hidden;
        padding: 2px var(--zui-space-sm);
        border: 1px solid transparent;
        border-radius: var(--zui-radius-full);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-xs);
        font-weight: var(--zui-type-weight-medium);
        line-height: var(--zui-type-leading-xs);
        white-space: nowrap;
        transition-property: color, box-shadow;
        transition-duration: var(--zui-motion-duration-normal);
        transition-timing-function: var(--zui-motion-ease-standard);
    }"

// A mark in a badge is smaller than one in a button, because the badge itself is: twelve
// pixels beside a twelve-pixel line rather than sixteen beside fourteen.
":scope > .zui-icon { pointer-events: none; --zui-icon-md: 12px; }"

":scope[data-variant=\"default\"] {
        background-color: var(--zui-color-primary);
        color: var(--zui-color-primary-foreground);
    }"
":scope[data-variant=\"secondary\"] {
        background-color: var(--zui-color-secondary);
        color: var(--zui-color-secondary-foreground);
    }"
// White rather than the destructive foreground token, for the reason a destructive button's
// label is white: that token is the pale red a destructive *message* is set in.
":scope[data-variant=\"destructive\"] {
        background-color: var(--zui-color-control-destructive-fill);
        color: #ffffff;
    }"
":scope[data-variant=\"outline\"] {
        border-color: var(--zui-color-border);
        color: var(--zui-color-foreground);
    }"
":scope[data-variant=\"ghost\"] { color: var(--zui-color-foreground); }"
":scope[data-variant=\"link\"] {
        color: var(--zui-color-primary);
        text-underline-offset: 4px;
    }"

":scope:focus-visible {
        border-color: var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
":scope[data-variant=\"destructive\"]:focus-visible {
        box-shadow: 0 0 0 3px var(--zui-color-control-ring-invalid);
    }"
":scope:invalid {
        border-color: var(--zui-color-destructive);
        box-shadow: 0 0 0 3px var(--zui-color-control-ring-invalid);
    }"

}
