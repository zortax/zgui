//! What a toggle looks like, in tokens.

use zgui::style;

style! { pub ToggleStyle =>
    // A minimum width rather than a width: a toggle holding one mark is square, and a toggle
    // holding a word grows past that rather than clipping it.
    ":scope {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: var(--zui-space-sm);
        height: 36px;
        min-width: 36px;
        padding: 0 var(--zui-space-sm);
        border: 1px solid transparent;
        border-radius: var(--zui-radius-md);
        background-color: transparent;
        color: var(--zui-color-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-medium);
        line-height: var(--zui-type-leading-sm);
        white-space: nowrap;
        transition-property: color, background-color, border-color, box-shadow;
        transition-duration: var(--zui-motion-duration-normal);
        transition-timing-function: var(--zui-motion-ease-standard);
    }"
    ":scope .zui-icon { pointer-events: none; flex: none; }"

    ":scope[data-size=\"sm\"] {
        height: 32px;
        min-width: 32px;
        padding: 0 calc(var(--zui-space-base) * 1.5);
    }"
    ":scope[data-size=\"lg\"] {
        height: 40px;
        min-width: 40px;
        padding: 0 calc(var(--zui-space-base) * 2.5);
    }"

    ":scope[data-variant=\"outline\"] {
        border-color: var(--zui-color-input);
        box-shadow: var(--zui-shadow-xs);
    }"

    // Off and hovered is the muted pair; on is the accent pair. The two are different colours on
    // purpose: a toggle that lit up under the pointer the same way it does when it is pressed
    // would be one nobody can read the state of without moving the pointer away first.
    ":scope:hover {
        background-color: var(--zui-color-muted);
        color: var(--zui-color-muted-foreground);
    }"
    ":scope[data-variant=\"outline\"]:hover {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"
    ":scope:checked {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"

    ":scope:focus-visible {
        border-color: var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
    ":scope:invalid {
        border-color: var(--zui-color-destructive);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-destructive) 20%, transparent);
    }"
    ":scope:disabled { opacity: 0.5; pointer-events: none; }"
}
