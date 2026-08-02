//! What a plain chooser looks like, in tokens.

use zgui::style;

style! { pub NativeSelectStyle =>
// The right-hand padding is wide enough to clear the mark, which is laid over it rather than
// laid out beside it: a chosen label long enough to reach the edge then runs under the mark
// instead of pushing it out of the control.
":scope {
        position: relative;
        display: inline-flex;
        align-items: center;
        width: fit-content;
        min-width: 0;
        height: 36px;
        padding: 8px 36px 8px 12px;
        border: 1px solid var(--zui-color-input);
        border-radius: var(--zui-radius-md);
        background-color: var(--zui-color-control-field);
        color: var(--zui-color-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        white-space: nowrap;
        box-shadow: var(--zui-shadow-xs);
        outline: none;
        transition-property: color, box-shadow, border-color;
        transition-duration: var(--zui-motion-duration-normal);
        transition-timing-function: var(--zui-motion-ease-standard);
    }"
":scope[data-size=\"sm\"] { height: 32px; padding-top: 4px; padding-bottom: 4px; }"
":scope:focus-visible {
        border-color: var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent),
                    var(--zui-shadow-xs);
    }"
":scope:hover { background-color: var(--zui-color-control-field-hover); }"
":scope:invalid { border-color: var(--zui-color-destructive); }"
":scope:invalid:focus-visible {
        border-color: var(--zui-color-destructive);
        box-shadow: 0 0 0 3px var(--zui-color-control-ring-invalid),
                    var(--zui-shadow-xs);
    }"
":scope:disabled { opacity: 0.5; pointer-events: none; }"
".zui-native-select__placeholder { color: var(--zui-color-muted-foreground); }"
// The mark is drawn by the chooser rather than shipped as a child, so a caller writing options
// does not have to remember to add one — and it takes no pointer events, so a click that lands
// on it still opens the list.
".zui-native-select__mark {
        position: absolute;
        top: 50%;
        right: 14px;
        transform: translateY(-50%);
        color: var(--zui-color-muted-foreground);
        opacity: 0.5;
        pointer-events: none;
        user-select: none;
    }"
".zui-native-select__list { padding: var(--zui-space-xs); min-width: 128px; overflow-y: auto; }"
// The rows are the select's rows, under this component's own names: the two lists have to look
// like the same control, because to the person choosing they are.
".zui-native-select__option {
        position: relative;
        display: flex;
        width: 100%;
        flex-direction: row;
        align-items: center;
        gap: var(--zui-space-sm);
        padding: 6px 32px 6px 8px;
        border-radius: var(--zui-radius-sm);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        outline: none;
        user-select: none;
    }"
// The highlight is `data-active`, not `:hover` and not focus: the caret never leaves the
// control, so there is nothing here for `:focus` to match and the option being walked has to
// be said out loud rather than implied.
".zui-native-select__option[data-active=\"true\"] {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"
".zui-native-select__option[data-disabled=\"true\"] { opacity: 0.5; pointer-events: none; }"
// The tick is laid over the right-hand padding rather than taking a column of its own, so a row
// with a tick and a row without it start their text in the same place.
".zui-native-select__indicator {
        position: absolute;
        right: 8px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 14px;
        height: 14px;
    }"
".zui-native-select__optgroup { display: flex; flex-direction: column; }"
".zui-native-select__optgroup-label {
        padding: 6px 8px;
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-xs);
        font-weight: var(--zui-type-weight-medium);
        color: var(--zui-color-muted-foreground);
        user-select: none;
    }"
}
