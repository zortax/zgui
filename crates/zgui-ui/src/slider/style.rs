//! What a slider looks like, in tokens.
//!
//! The thumb's ring is four pixels rather than the three every other control here wears, and it is
//! drawn on hover as well as on focus. Both are the slider's own: the thumb is the smallest target
//! in the library, so the halo is what makes it findable with a pointer, and a three-pixel band
//! around sixteen pixels of circle reads as a thicker border rather than as a ring.

use zgui::style;

style! { pub SliderStyle =>
    ":scope {
        position: relative;
        display: flex;
        flex-direction: row;
        align-items: center;
        height: 16px;
        width: 100%;
        outline: none;
        user-select: none;
    }"
    ":scope > .zui-slider__track {
        position: relative;
        height: 6px;
        width: 100%;
        overflow: hidden;
        border-radius: var(--zui-radius-full);
        background-color: var(--zui-color-muted);
    }"
    ":scope > .zui-slider__track > .zui-slider__range {
        height: 100%;
        width: var(--zui-slider-fraction, 0%);
        border-radius: inherit;
        background-color: var(--zui-color-primary);
    }"
    // White rather than the page's surface, so the thumb keeps its edge against the filled part of
    // the track in either scheme — a dark-surface thumb on a dark track is a hole, not a handle.
    ":scope > .zui-slider__thumb {
        position: absolute;
        left: var(--zui-slider-fraction, 0%);
        width: 16px;
        height: 16px;
        margin-left: -8px;
        flex: none;
        border: 1px solid var(--zui-color-primary);
        border-radius: var(--zui-radius-full);
        background-color: oklch(1 0 0);
        box-shadow: var(--zui-shadow-sm);
        transition-property: box-shadow;
        transition-duration: var(--zui-motion-duration-normal);
        transition-timing-function: var(--zui-motion-ease-standard);
    }"
    ":scope:hover > .zui-slider__thumb, :scope:focus-visible > .zui-slider__thumb {
        box-shadow: 0 0 0 4px color-mix(in oklab, var(--zui-color-ring) 50%, transparent),
                    var(--zui-shadow-sm);
    }"
    ":scope:disabled { opacity: 0.5; pointer-events: none; }"
}
