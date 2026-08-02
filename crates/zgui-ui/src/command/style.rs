//! What a command palette looks like, in tokens.

use zgui::style;

style! { pub CommandStyle =>
    // A palette is a panel the colour of a popover with a field at the top of it and a list under.
    // It takes whatever room it is given — inside a dialog that is the dialog, and on a page it is
    // whatever the page allotted — which is why nothing here states a width.
    ":scope {
        display: flex;
        flex-direction: column;
        width: 100%;
        height: 100%;
        overflow: hidden;
        border-radius: var(--zui-radius-md);
        background-color: var(--zui-color-popover);
        color: var(--zui-color-popover-foreground);
    }"

    // The field, with the rule under it that divides the question from the answers. It is one row
    // tall and the field inside it is transparent: what the eye reads as the search box is this
    // whole band, and a second box drawn inside it would be a box inside a box.
    ".zui-command__field {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: var(--zui-space-sm);
        height: 36px;
        padding: 0 var(--zui-space-md);
        border-bottom: 1px solid var(--zui-color-border);
    }"
    // The magnifier is a hint rather than a control, so it is drawn at half strength.
    ".zui-command__field .zui-icon { opacity: 0.5; flex-shrink: 0; }"
    // The field inside the band gives up every mark of being a field: its border, its background,
    // its own focus ring. There is exactly one thing to type into on this surface and the caret is
    // already in it, so a ring drawn round it would outline the obvious.
    ".zui-command__field .zui-command__input {
        flex: 1 1 auto;
        height: 40px;
        padding: 0;
        border: none;
        background-color: transparent;
        box-shadow: none;
        outline: none;
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
    }"
    ".zui-command__field .zui-command__input:focus,
     .zui-command__field .zui-command__input:focus-visible {
        outline: none;
        box-shadow: none;
        border: none;
    }"

    // Three hundred pixels of list, and after that it scrolls. A palette taller than that stops
    // being something the eye can take in and becomes a page.
    ".zui-command__list {
        max-height: 300px;
        overflow-x: hidden;
        overflow-y: auto;
        padding: var(--zui-space-xs);
        gap: 0;
    }"
    ".zui-command__group { display: flex; flex-direction: column; gap: 0; padding: var(--zui-space-xs); }"
    // A group's heading is quieter than a menu's, because a palette is read by typing rather than
    // by scanning: the headings are there to say what *kind* of thing was found, not to be aimed at.
    ".zui-command__group .zui-menu__label {
        font-size: var(--zui-type-size-xs);
        line-height: var(--zui-type-leading-xs);
        color: var(--zui-color-muted-foreground);
    }"
    // "Nothing by that name" is the answer to a question, so it sits where the answers would have
    // been — centred in the list, with room above and below it rather than tucked against a corner.
    ":scope .zui-combobox__empty {
        padding: var(--zui-space-xl) 0;
        text-align: center;
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        color: inherit;
    }"

    // In a dialog the palette *is* the dialog: the panel's own padding goes, so the field's rule
    // reaches both edges, and the field grows a third taller — a palette raised deliberately from
    // a keystroke is the thing being used, not a control on something else.
    ".zui-dialog.zui-command__dialog { padding: 0; gap: 0; overflow: hidden; }"
    ".zui-command__dialog .zui-command__field { height: 48px; }"
    ".zui-command__dialog .zui-command__group { padding-left: var(--zui-space-sm); padding-right: var(--zui-space-sm); }"
}
