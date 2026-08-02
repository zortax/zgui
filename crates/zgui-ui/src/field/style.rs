//! What a field and the things around it look like, in tokens.
//!
//! Three sheets rather than one, because the three are installed by different components and a
//! component installs only what it draws — a form of plain fields never loads the rules for a set,
//! a legend or a separator it does not have.

use zgui::style;

style! { pub FieldStyle =>
    ":scope { display: flex; width: 100%; gap: var(--zui-space-md); }"
    ":scope[data-orientation=\"vertical\"] { flex-direction: column; }"
    // A stacked field's pieces are as wide as the field, so a control does not sit in a column of
    // its own with the label's leftover width beside it.
    ":scope[data-orientation=\"vertical\"] > * { width: 100%; }"
    ":scope[data-orientation=\"horizontal\"] { flex-direction: row; align-items: center; }"
    // Wrong colours the whole field, so the label goes red with the message under it rather than
    // the message being the only part that changed.
    ":scope[data-invalid=\"true\"] { color: var(--zui-color-destructive); }"
    ":scope[data-disabled=\"true\"] { opacity: 0.5; pointer-events: none; }"
    // Tighter than the gap between one field and the next: a title and the line qualifying it are
    // one thing said twice, and the space between them has to read as smaller than the space
    // around the pair.
    ".zui-field__content {
        display: flex;
        flex: 1;
        flex-direction: column;
        gap: 6px;
        line-height: 1.375;
    }"
}

style! { pub FieldGroupStyle =>
    ":scope {
        display: flex;
        width: 100%;
        flex-direction: column;
        gap: calc(var(--zui-space-base) * 7);
    }"
    // A group inside a group is a subsection, and its fields sit closer together than the sections
    // do — which is what makes the nesting visible without a rule or a heading.
    ":scope .zui-field__group { gap: var(--zui-space-lg); }"
    ".zui-field__set {
        display: flex;
        flex-direction: column;
        gap: var(--zui-space-xl);
        border: 0;
        margin: 0;
        padding: 0;
    }"
    ".zui-field__legend {
        display: block;
        margin-bottom: var(--zui-space-md);
        font-family: var(--zui-type-family-sans);
        font-weight: var(--zui-type-weight-medium);
    }"
    ".zui-field__legend[data-variant=\"legend\"] {
        font-size: var(--zui-type-size-md);
        line-height: var(--zui-type-leading-md);
    }"
    ".zui-field__legend[data-variant=\"label\"] {
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
    }"
}

style! { pub FieldTextStyle =>
    // As wide as its words rather than as wide as the field, so that pressing beside a label is not
    // pressing the label — which would move the keyboard to a control the pointer never touched.
    ".zui-field__label {
        display: flex;
        width: fit-content;
        align-items: center;
        gap: var(--zui-space-sm);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-medium);
        line-height: 1.375;
        user-select: none;
    }"
    ".zui-field__title {
        display: flex;
        width: fit-content;
        align-items: center;
        gap: var(--zui-space-sm);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-medium);
        line-height: 1.375;
    }"
    ".zui-field__description {
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-normal);
        line-height: 1.5;
        color: var(--zui-color-muted-foreground);
    }"
    ".zui-field__error {
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-normal);
        line-height: var(--zui-type-leading-sm);
        color: var(--zui-color-destructive);
    }"
    // The rule is drawn across the middle of a box the height of one line and pulled back into the
    // gaps on either side, so a separator with a word in it takes up no more room in the stack than
    // a bare one — and the word sits on the page's own colour, which is what breaks the line.
    ".zui-field__separator {
        position: relative;
        height: 20px;
        margin: -8px 0;
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
    }"
    ".zui-field__separator > .zui-field__rule {
        position: absolute;
        top: 50%;
        left: 0;
        right: 0;
        height: 1px;
        background-color: var(--zui-color-border);
    }"
    ".zui-field__separator > .zui-field__separator-content {
        position: relative;
        display: block;
        width: fit-content;
        margin: 0 auto;
        padding: 0 var(--zui-space-sm);
        background-color: var(--zui-color-background);
        color: var(--zui-color-muted-foreground);
    }"
}
