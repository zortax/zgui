//! What a radio group looks like, in tokens.

use zgui::style;

style! { pub RadioGroupStyle =>
    ":scope { display: flex; flex-direction: column; gap: var(--zui-space-md); }"
    ":scope[data-orientation=\"horizontal\"] { flex-direction: row; }"
}

style! { pub RadioItemStyle =>
// The colour is the group's tint rather than the page's foreground, so the disc inside inherits
// it and a re-tinted interface marks its chosen choice in the tint.
":scope {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 16px;
        height: 16px;
        flex: none;
        border: 1px solid var(--zui-color-input);
        border-radius: var(--zui-radius-full);
        background-color: var(--zui-color-control-field);
        color: var(--zui-color-primary);
        box-shadow: var(--zui-shadow-xs);
        outline: none;
        transition-property: color, box-shadow, border-color;
        transition-duration: var(--zui-motion-duration-normal);
        transition-timing-function: var(--zui-motion-ease-standard);
    }"
":scope:focus-visible {
        border-color: var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent),
                    var(--zui-shadow-xs);
    }"
":scope:invalid { border-color: var(--zui-color-destructive); }"
":scope:invalid:focus-visible {
        border-color: var(--zui-color-destructive);
        box-shadow: 0 0 0 3px var(--zui-color-control-ring-invalid),
                    var(--zui-shadow-xs);
    }"
":scope:disabled { opacity: 0.5; pointer-events: none; }"
// The dot the reference draws is eight pixels across, and the disc glyph inks half its own box —
// its circle spans twelve of the twenty-four view-box units — so the box stays at the default
// sixteen and the ink comes out at eight. Pinned through `--zui-icon-md`, which is the channel
// the icon's own sheet reads, rather than `--zui-icon-size`, which it sets on itself; and pinned
// at all so a surrounding control that narrowed the channel cannot shrink the dot with it.
":scope > .zui-icon { opacity: 0; --zui-icon-md: 16px; }"
":scope:checked > .zui-icon { opacity: 1; }"
}
