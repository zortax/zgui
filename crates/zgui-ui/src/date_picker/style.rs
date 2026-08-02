//! What a date picker looks like, in tokens.

use zgui::style;

style! { pub DatePickerStyle =>
    ":scope {
        display: inline-flex;
        flex-direction: column;
        gap: var(--zui-space-xs);
    }"
    // The trigger is an outlined button that happens to say a date: the name on the left, the sign
    // that it opens something on the right, and the width fixed so the field does not jump between
    // the placeholder and a long date.
    ":scope .zui-date-picker__trigger {
        display: inline-flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--zui-space-sm);
        width: 192px;
        height: 36px;
        padding: 0 var(--zui-space-md);
        border: 1px solid var(--zui-color-border);
        border-radius: var(--zui-radius-md);
        background-color: var(--zui-color-background);
        box-shadow: var(--zui-shadow-xs);
        color: var(--zui-color-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-normal);
        line-height: var(--zui-type-leading-sm);
        text-align: left;
        white-space: nowrap;
        transition: background-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    border-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ":scope .zui-date-picker__trigger:hover {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"
    ":scope .zui-date-picker__trigger:focus-visible {
        border-color: var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
    ":scope .zui-date-picker__trigger:disabled {
        opacity: 0.5;
        pointer-events: none;
    }"
    ":scope .zui-date-picker__trigger[data-empty=\"true\"] {
        color: var(--zui-color-muted-foreground);
    }"
    ":scope .zui-date-picker__label { overflow: hidden; text-overflow: ellipsis; }"
    // The surface is portaled out of the picker, so this is a plain rule rather than a scoped one.
    // It has no padding of its own: the calendar brings its own, and a surface that added more
    // would put a band of popover colour around a calendar that already ends in one.
    ".zui-date-picker__surface {
        width: auto;
        padding: 0;
        overflow: hidden;
    }"
}
