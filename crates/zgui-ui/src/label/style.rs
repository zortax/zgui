//! What a label looks like, in tokens.

use zgui::style;

style! { pub LabelStyle =>
    // A row rather than a line of text, because a label is often a word and a mark — a required
    // asterisk, a help sign, a small badge — and those want to sit on the word's own baseline box
    // rather than on the text baseline.
    //
    // The line height is the type size exactly. A label is the top line of a stacked field, and
    // any leading above it becomes a gap between the label and the control that the field's own
    // spacing did not ask for.
    ":scope {
        display: flex;
        align-items: center;
        gap: var(--zui-space-sm);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-medium);
        line-height: 1;
        color: var(--zui-color-foreground);
        user-select: none;
    }"
    // A label whose control is disabled is faded with it. There is no prop for that and no signal
    // behind it: the state is the control's, and this is a sibling selector reading it.
    ":scope[data-disabled] { opacity: 0.5; pointer-events: none; }"
}
