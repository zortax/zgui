//! What a carousel looks like, in tokens.

use zgui::style;

/// The custom property carrying which slide is showing, counted from zero.
pub(crate) const INDEX: &str = "zui-carousel-index";

/// The custom property carrying how far the track has travelled, as a negative length in CSS
/// pixels.
pub(crate) const OFFSET: &str = "zui-carousel-offset";

style! { pub CarouselStyle =>
    ":scope { position: relative; }"
    // `min-width: 0` because the track is as long as every slide put together: without it the
    // viewport's automatic minimum size is the whole strip, the arrows are pushed past the edge of
    // whatever holds the carousel, and neither of them is ever drawn.
    ".zui-carousel__viewport { min-width: 0; min-height: 0; overflow: hidden; }"
    // Relative positioning rather than a translation, and the difference is whether the slides are
    // drawn at all. A viewport that clips discards what falls outside it *before* a paint-time
    // transform is applied, so a strip moved by `transform` brings nothing back into the frame: the
    // slide that ought to have arrived was thrown away for being where it was laid out. A relative
    // offset moves the track in the same space the clip is measured in, so the slide that arrives
    // is inside the viewport by the time anything asks what to keep.
    //
    // It is a length rather than a percentage because a percentage `left` resolves against the
    // viewport, which is one slide only when a viewport holds exactly one.
    //
    // The negative margin and the slides' matching padding are the gutter between slides: room
    // taken *inside* each slide, so the strip still starts flush with the viewport's leading edge
    // and no slide is a gap's width narrower than the rest.
    ".zui-carousel__track {
        position: relative;
        display: flex;
        flex-direction: row;
        margin-left: calc(var(--zui-space-lg) * -1);
        left: var(--zui-carousel-offset, 0px);
        transition: left var(--zui-motion-duration-slow) var(--zui-motion-ease-standard);
    }"
    ":scope[data-orientation=\"vertical\"] .zui-carousel__track {
        flex-direction: column;
        margin-left: 0;
        margin-top: calc(var(--zui-space-lg) * -1);
        left: 0;
        top: var(--zui-carousel-offset, 0px);
        transition: top var(--zui-motion-duration-slow) var(--zui-motion-ease-standard);
    }"
    ".zui-carousel__item {
        flex: 0 0 100%;
        min-width: 0;
        padding-left: var(--zui-space-lg);
    }"
    ":scope[data-orientation=\"vertical\"] .zui-carousel__item {
        padding-left: 0;
        padding-top: var(--zui-space-lg);
    }"

    // Outside the viewport rather than beside it, so the strip is the full width of whatever holds
    // the carousel and the arrows hang in the margin either side of it.
    ".zui-carousel__arrow {
        position: absolute;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: calc(var(--zui-space-base) * 8);
        height: calc(var(--zui-space-base) * 8);
        padding: 0;
        border: 1px solid var(--zui-color-border);
        border-radius: var(--zui-radius-full);
        background-color: var(--zui-color-background);
        color: var(--zui-color-foreground);
        box-shadow: var(--zui-shadow-xs);
        transition: background-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ":scope[data-orientation=\"horizontal\"] .zui-carousel__arrow--previous {
        top: 50%;
        left: calc(var(--zui-space-base) * -12);
        transform: translateY(-50%);
    }"
    ":scope[data-orientation=\"horizontal\"] .zui-carousel__arrow--next {
        top: 50%;
        right: calc(var(--zui-space-base) * -12);
        transform: translateY(-50%);
    }"
    // The same two arrows turned a quarter, rather than a second pair pointing up and down: one
    // drawing that has been rotated cannot disagree with itself about which way is forward.
    ":scope[data-orientation=\"vertical\"] .zui-carousel__arrow--previous {
        top: calc(var(--zui-space-base) * -12);
        left: 50%;
        transform: translateX(-50%) rotate(90deg);
    }"
    ":scope[data-orientation=\"vertical\"] .zui-carousel__arrow--next {
        bottom: calc(var(--zui-space-base) * -12);
        left: 50%;
        transform: translateX(-50%) rotate(90deg);
    }"
    ".zui-carousel__arrow:hover {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"
    ".zui-carousel__arrow:focus-visible {
        outline: none;
        border-color: var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
    ".zui-carousel__arrow:disabled { opacity: 0.5; pointer-events: none; }"
}
