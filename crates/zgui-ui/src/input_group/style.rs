//! What a field with things attached to it looks like, in tokens.

use zgui::style;

style! { pub InputGroupStyle =>
// The border, the rounding and the ring belong to the group rather than to the field inside
// it: one outline around the lot is what makes a field and its button read as one control, and
// a field that kept its own would draw a second box inside the first.
//
// The height is a floor rather than a measurement. A group holding a one-line field is exactly
// the height of that field, and a group holding a text area or a row of controls of its own
// grows to whatever those need — with no rule anywhere asking what is inside it.
":scope {
        position: relative;
        display: flex;
        width: 100%;
        min-width: 0;
        min-height: 36px;
        align-items: center;
        flex-wrap: wrap;
        border: 1px solid var(--zui-color-input);
        border-radius: var(--zui-radius-md);
        background-color: var(--zui-color-control-field);
        box-shadow: var(--zui-shadow-xs);
        outline: none;
        transition-property: color, box-shadow, border-color;
        transition-duration: var(--zui-motion-duration-normal);
        transition-timing-function: var(--zui-motion-ease-standard);
    }"
// `:focus-within` rather than a rule about which descendant has focus: the group's job is to
// show that the thing it frames is being typed into, and it does not care which of its pieces
// that is.
":scope:focus-within {
        border-color: var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent),
                    var(--zui-shadow-xs);
    }"
":scope[data-invalid=\"true\"] {
        border-color: var(--zui-color-destructive);
        box-shadow: 0 0 0 3px var(--zui-color-control-ring-invalid),
                    var(--zui-shadow-xs);
    }"
":scope[data-disabled=\"true\"] { opacity: 0.5; pointer-events: none; }"
}

style! { pub InputGroupPartStyle =>
// The cursor is a text caret over the whole strip, including the parts that are not the field,
// because pressing a mark beside a field puts the caret in the field.
".zui-input-group__addon {
        display: flex;
        height: auto;
        align-items: center;
        justify-content: center;
        gap: var(--zui-space-sm);
        padding: 6px 0;
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-medium);
        line-height: var(--zui-type-leading-sm);
        color: var(--zui-color-muted-foreground);
        cursor: text;
        user-select: none;
    }"
".zui-input-group__addon[data-align=\"inline-start\"] { order: -1; padding-left: 12px; }"
".zui-input-group__addon[data-align=\"inline-end\"] { order: 1; padding-right: 12px; }"
// A control pulls its own edge back into the group's padding, so that the *drawing* inside the
// button lines up with where a mark would have been rather than the button's box doing.
".zui-input-group__addon[data-align=\"inline-start\"] > .zui-button { margin-left: -7.2px; }"
".zui-input-group__addon[data-align=\"inline-end\"] > .zui-button { margin-right: -7.2px; }"
".zui-input-group__addon[data-align=\"inline-start\"] > .zui-kbd-group { margin-left: -5.6px; }"
".zui-input-group__addon[data-align=\"inline-end\"] > .zui-kbd-group { margin-right: -5.6px; }"
".zui-input-group__addon[data-align=\"block-start\"] {
        order: -1;
        width: 100%;
        justify-content: flex-start;
        padding: 12px 12px 0 12px;
    }"
".zui-input-group__addon[data-align=\"block-end\"] {
        order: 1;
        width: 100%;
        justify-content: flex-start;
        padding: 0 12px 12px 12px;
    }"
".zui-input-group__text {
        display: flex;
        align-items: center;
        gap: var(--zui-space-sm);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        color: var(--zui-color-muted-foreground);
    }"
// Everything the group has already drawn is taken away here rather than never asked for: the
// field is an ordinary field that a group has stripped, so the same component serves both.
//
// `width: auto` is part of the stripping. A field on its own fills its row by saying `width:
// 100%`, and inside a wrapping group that full width is a demand for a whole flex line — the
// field drops below an inline addon instead of sharing the row with it. Growing from a zero
// basis says the same thing the field meant, in terms the row can satisfy beside its addons.
".zui-field.zui-input-group__field {
        flex: 1 1 0%;
        width: auto;
        min-width: 0;
        border: 0;
        border-radius: 0;
        background-color: transparent;
        box-shadow: none;
        outline: none;
    }"
".zui-field.zui-input-group__field:focus-visible { box-shadow: none; border-color: transparent; }"
".zui-field.zui-input-group__area {
        flex: 1 1 0%;
        width: auto;
        min-width: 0;
        resize: none;
        border: 0;
        border-radius: 0;
        background-color: transparent;
        box-shadow: none;
        outline: none;
        padding-top: 12px;
        padding-bottom: 12px;
    }"
".zui-field.zui-input-group__area:focus-visible {
        box-shadow: none;
        border-color: transparent;
    }"
// A group's own controls are smaller than a button in a page and carry no lift of their own,
// because they sit inside a box that is already raised. The lift is cleared through the
// button's own custom property rather than through `box-shadow`, which the same declaration
// draws its focus ring with.
".zui-button.zui-input-group__button { --zui-button-lift: 0 0 transparent; }"
".zui-button.zui-input-group__button[data-size=\"xs\"] {
        height: 24px;
        gap: var(--zui-space-xs);
        padding: 0 8px;
        border-radius: calc(var(--zui-radius-base) - 5px);
        --zui-icon-md: 14px;
    }"
".zui-button.zui-input-group__button[data-size=\"sm\"] {
        height: 32px;
        gap: calc(var(--zui-space-base) * 1.5);
        padding: 0 10px;
        border-radius: var(--zui-radius-md);
    }"
".zui-button.zui-input-group__button[data-size=\"icon-xs\"] {
        width: 24px;
        height: 24px;
        padding: 0;
        border-radius: calc(var(--zui-radius-base) - 5px);
        --zui-icon-md: 14px;
    }"
".zui-button.zui-input-group__button[data-size=\"icon-sm\"] {
        width: 32px;
        height: 32px;
        padding: 0;
        border-radius: var(--zui-radius-md);
    }"
// The group has the fill on a dark page, so the field inside it keeps none: two faint fills one
// inside the other read as a box with a lighter box in it.
}
