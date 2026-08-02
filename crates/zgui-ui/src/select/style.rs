//! What a select looks like, in tokens.

use zgui::style;

style! { pub SelectStyle =>
// As wide as its content, not as wide as its row. A select is a control in a line of other
// controls far more often than it is a field filling a form, and a caller who wants the second
// says so with one declaration.
":scope {
        display: inline-flex;
        flex-direction: row;
        align-items: center;
        justify-content: space-between;
        gap: var(--zui-space-sm);
        width: fit-content;
        height: 36px;
        padding: 8px 12px;
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
":scope[data-size=\"sm\"] { height: 32px; }"
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
// The chevron sits still while the list opens. It marks *this control opens something*, which
// is as true closed as open, and a mark that spins says the surface is coming from the mark.
".zui-select__chevron { opacity: 0.5; pointer-events: none; }"
".zui-select__placeholder { color: var(--zui-color-muted-foreground); }"
".zui-select__list { padding: var(--zui-space-xs); min-width: 128px; overflow-y: auto; }"
// The tick is laid over the right-hand padding rather than taking a column of its own, so a row
// with a tick and a row without it start their text in the same place.
".zui-select__item {
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
// trigger, so there is nothing here for `:focus` to match and the option being walked has to
// be said out loud rather than implied.
".zui-select__item[data-active=\"true\"] {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"
".zui-select__item[data-disabled=\"true\"] { opacity: 0.5; pointer-events: none; }"
".zui-select__indicator {
        position: absolute;
        right: 8px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 14px;
        height: 14px;
    }"
}
