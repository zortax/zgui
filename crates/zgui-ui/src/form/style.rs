//! What a form looks like, in tokens.

use zgui::style;

style! { pub FormStyle =>
    ":scope { display: flex; flex-direction: column; gap: var(--zui-space-xl); }"
    ".zui-form__field { display: flex; flex-direction: column; }"
    // One field's pieces sit closer together than one field sits to the next, which is what makes a
    // label, a control and a line of help read as one question rather than as three rows.
    ".zui-form__item { display: flex; flex-direction: column; gap: var(--zui-space-sm); }"
    ".zui-form__label[data-invalid=\"true\"] { color: var(--zui-color-destructive); }"
    ".zui-form__description {
        color: var(--zui-color-muted-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
    }"
    ".zui-form__message {
        color: var(--zui-color-destructive);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
    }"
    // Quiet rather than gone: the element is what the control's description points at, and a
    // relation to something that is sometimes not there is a relation that is sometimes wrong.
    ".zui-form__message[data-state=\"quiet\"] { display: none; }"
}
