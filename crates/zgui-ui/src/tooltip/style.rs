//! What a tooltip looks like, in tokens.

use zgui::style;

style! { pub TooltipStyle =>
    // A tooltip is the one surface here that inverts. Everything else in this library floats a
    // panel the colour of the page and separates it with a border and a shadow; a tooltip is a
    // solid slug of the foreground colour with the background written on it, which is what makes
    // three words legible against whatever they happen to be over without any edge at all.
    //
    // It takes the width of what is in it. A tooltip long enough to need wrapping is a description
    // and belongs in a hover card.
    // `position: relative` is what puts the arrow *inside* this box rather than beside it. An
    // absolutely positioned box is laid out and painted against its containing block, so with a
    // static surface the arrow's containing block would be the positioner around it — one box
    // further out than the panel it belongs to. Everything the surface then does to itself as a
    // whole, the arrow does not do: the exit fades the slug away and leaves the diamond hanging in
    // the air at full strength until the whole thing is unmounted.
    //
    // It is also the honest reading of the arrow's own offsets. `left: 50%` is meant to be half of
    // the tooltip, and that it happened to be half of the positioner as well was an accident of the
    // two boxes being the same width.
    //
    // A tooltip's motion is the shortest in the library and deliberately so. It is not a surface
    // somebody asked for, it is a name arriving under a pointer that is already there, and anything
    // long enough to be *watched* arriving reads as lag rather than as polish — so the fade is over
    // in a couple of frames and the slug does not zoom at all, leaving the eight-pixel drift
    // towards the trigger as the whole of the movement.
    ":scope {
        position: relative;
        width: fit-content;
        padding: calc(var(--zui-space-base) * 1.5) var(--zui-space-md);
        --zui-surface-border: none;
        --zui-surface-radius: var(--zui-radius-md);
        --zui-surface-shadow: none;
        --zui-surface-fill: var(--zui-color-foreground);
        --zui-surface-ink: var(--zui-color-background);
        --zui-surface-enter-duration: 60ms;
        --zui-surface-exit-duration: 60ms;
        --zui-surface-enter-scale: 1;
        --zui-surface-exit-scale: 1;
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-xs);
        line-height: var(--zui-type-leading-xs);
    }"

    // A trigger is a wrapper around whatever it is describing, and has no appearance of its own.
    ".zui-tooltip__trigger { display: inline-flex; align-items: center; }"

    // The point that ties the slug to what it names: a square of the same colour turned on its
    // corner, half of it outside the surface and half behind it. Half *plus two pixels* behind, so
    // that the join between the two is under the surface rather than on its edge — a diamond that
    // met the edge exactly would show a hairline seam wherever the two rasterise a fraction apart.
    ".zui-tooltip__arrow {
        position: absolute;
        width: 10px;
        height: 10px;
        border-radius: 2px;
        background-color: var(--zui-color-foreground);
    }"
    ".zui-overlay-positioner[data-side=\"top\"] .zui-tooltip__arrow {
        left: 50%;
        bottom: 0;
        transform: translate(-50%, calc(50% - 2px)) rotate(45deg);
    }"
    ".zui-overlay-positioner[data-side=\"bottom\"] .zui-tooltip__arrow {
        left: 50%;
        top: 0;
        transform: translate(-50%, calc(-50% + 2px)) rotate(45deg);
    }"
    ".zui-overlay-positioner[data-side=\"left\"] .zui-tooltip__arrow {
        top: 50%;
        right: 0;
        transform: translate(calc(50% - 2px), -50%) rotate(45deg);
    }"
    ".zui-overlay-positioner[data-side=\"right\"] .zui-tooltip__arrow {
        top: 50%;
        left: 0;
        transform: translate(calc(-50% + 2px), -50%) rotate(45deg);
    }"
}
