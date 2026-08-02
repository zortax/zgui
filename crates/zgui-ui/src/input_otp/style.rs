//! What a one-time-code field looks like, in tokens.

use zgui::style;

style! { pub InputOtpStyle =>
":scope {
        display: inline-flex;
        flex-direction: row;
        align-items: center;
        gap: var(--zui-space-sm);
        outline: none;
    }"
":scope:disabled { opacity: 0.5; pointer-events: none; }"
// A run of boxes with no gap between them, so the two share one line rather than each drawing
// its own. The gap above is between *runs*, which is what the dash sits in.
".zui-otp__group { display: flex; flex-direction: row; align-items: center; }"
".zui-otp__separator {
        display: flex;
        align-items: center;
        justify-content: center;
        color: var(--zui-color-muted-foreground);
    }"
// No left border, and the first box of a run puts one back: two boxes side by side then meet on
// one hairline instead of two, which is the difference between a strip and a row of tiles.
".zui-otp__slot {
        position: relative;
        display: flex;
        align-items: center;
        justify-content: center;
        width: 36px;
        height: 36px;
        border: 1px solid var(--zui-color-input);
        border-left-width: 0;
        background-color: var(--zui-color-control-field);
        color: var(--zui-color-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        box-shadow: var(--zui-shadow-xs);
        outline: none;
        transition-property: box-shadow, border-color;
        transition-duration: var(--zui-motion-duration-normal);
        transition-timing-function: var(--zui-motion-ease-standard);
    }"
".zui-otp__slot:first-child {
        border-left-width: 1px;
        border-top-left-radius: var(--zui-radius-md);
        border-bottom-left-radius: var(--zui-radius-md);
    }"
".zui-otp__slot:last-child {
        border-top-right-radius: var(--zui-radius-md);
        border-bottom-right-radius: var(--zui-radius-md);
    }"
// The slot the next character goes into is marked by the group, and the ring is drawn on it
// rather than on the group: the group is what has focus, and a focus ring around all six
// boxes says nothing about where typing lands. It is lifted a layer so that the ring lies over
// the boxes on either side rather than under them.
":scope:focus .zui-otp__slot[data-active] {
        z-index: 1;
        border-color: var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent),
                    var(--zui-shadow-xs);
    }"
":scope:invalid .zui-otp__slot { border-color: var(--zui-color-destructive); }"
":scope:invalid:focus .zui-otp__slot[data-active] {
        box-shadow: 0 0 0 3px var(--zui-color-control-ring-invalid),
                    var(--zui-shadow-xs);
    }"
// A caret this component draws, unlike every other field here: there is no editing model behind
// these boxes to draw one from, because the value is held whole and typed at from a key
// handler. It shows only while the field has focus, so an unfocused code does not appear to be
// waiting for a keystroke.
".zui-otp__caret {
        position: absolute;
        top: 0;
        right: 0;
        bottom: 0;
        left: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        opacity: 0;
        pointer-events: none;
    }"
":scope:focus .zui-otp__slot[data-active] > .zui-otp__caret { opacity: 1; }"
".zui-otp__caret > .zui-otp__bar {
        width: 1px;
        height: 16px;
        background-color: var(--zui-color-foreground);
        animation: zui-otp-caret-blink 1250ms var(--zui-motion-ease-out) infinite;
    }"
// Lit for most of the cycle and dark for a short stretch in the middle of it, which is what a
// caret does — an even on-off blink reads as a flashing box rather than as a place to type.
"@keyframes zui-otp-caret-blink {
        0% { opacity: 1; }
        20% { opacity: 0; }
        50% { opacity: 0; }
        70% { opacity: 1; }
        100% { opacity: 1; }
    }"
}
