//! What a tooltip looks like, in tokens.

use zgui::style;

style! { pub TooltipStyle =>
    // A tooltip takes its fill, its text and its edge from the tooltip tokens. By default these
    // invert the page: a solid slug of the foreground colour with the background written on it.
    // A theme that sets the tokens to a surface tone gets a same-tone tooltip with a visible edge.
    //
    // It takes the width of what is in it. A tooltip long enough to need wrapping is a description
    // and belongs in a hover card.
    // `position: relative` is what puts the arrow *inside* this box rather than beside it. An
    // absolutely positioned box is laid out and painted against its containing block, so with a
    // static surface the arrow's containing block would be the positioner around it — one box
    // further out than the panel it belongs to.
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
        padding: calc(var(--zui-space-base) * 1.5 - 1px) calc(var(--zui-space-md) - 1px);
        --zui-surface-border: 1px solid var(--zui-color-tooltip-border);
        --zui-surface-radius: var(--zui-radius-md);
        --zui-surface-shadow: none;
        --zui-surface-fill: var(--zui-color-tooltip);
        --zui-surface-ink: var(--zui-color-tooltip-foreground);
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

    // The point that ties the slug to what it names: a square of the same fill turned on its
    // corner. Its centre sits on the inner line of the tooltip's edge, so its inner half lies
    // over the surface and its side corners meet that edge. Only the two outer sides carry the
    // edge colour, which continues the tooltip's edge around the point.
    ".zui-tooltip__arrow {
        position: absolute;
        width: 10px;
        height: 10px;
        border: 0 solid var(--zui-color-tooltip-border);
        border-radius: 2px;
        background-color: var(--zui-color-tooltip);
    }"
    // The arrow points at the centre of the trigger, which the positioner publishes. It keeps
    // clear of the rounded corners of the panel.
    ".zui-overlay-positioner[data-side=\"top\"] .zui-tooltip__arrow {
        left: clamp(10px, var(--zui-popper-anchor-x, 50%), calc(100% - 10px));
        bottom: 0;
        border-right-width: 1px;
        border-bottom-width: 1px;
        transform: translate(-50%, 50%) rotate(45deg);
    }"
    ".zui-overlay-positioner[data-side=\"bottom\"] .zui-tooltip__arrow {
        left: clamp(10px, var(--zui-popper-anchor-x, 50%), calc(100% - 10px));
        top: 0;
        border-top-width: 1px;
        border-left-width: 1px;
        transform: translate(-50%, -50%) rotate(45deg);
    }"
    ".zui-overlay-positioner[data-side=\"left\"] .zui-tooltip__arrow {
        top: clamp(10px, var(--zui-popper-anchor-y, 50%), calc(100% - 10px));
        right: 0;
        border-top-width: 1px;
        border-right-width: 1px;
        transform: translate(50%, -50%) rotate(45deg);
    }"
    ".zui-overlay-positioner[data-side=\"right\"] .zui-tooltip__arrow {
        top: clamp(10px, var(--zui-popper-anchor-y, 50%), calc(100% - 10px));
        left: 0;
        border-bottom-width: 1px;
        border-left-width: 1px;
        transform: translate(-50%, -50%) rotate(45deg);
    }"

    // The arrow arrives and leaves with the slug it belongs to, on the slug's own two durations.
    // It carries the fade itself because it is the one part of a tooltip drawn *past* the panel's
    // edge, and a frame redraws the rectangle an element covers: the strip of diamond outside that
    // rectangle keeps whatever was last painted there unless the diamond is animating as well.
    //
    // Opacity alone, so the turn and the placement each side gives it stand through both.
    ".zui-tooltip__arrow {
        animation: zui-tooltip-arrow-enter
            var(--zui-surface-enter-duration, var(--zui-motion-duration-normal))
            var(--zui-surface-enter-ease, ease) both;
    }"
    ".zui-surface[data-state=\"closed\"] .zui-tooltip__arrow {
        animation: zui-tooltip-arrow-exit
            var(--zui-surface-exit-duration, var(--zui-motion-duration-normal))
            var(--zui-surface-exit-ease, ease) both;
    }"
    // A tooltip taken down to show the next one goes at once, and so does its arrow.
    ":scope.zui-surface[data-instant] { animation: none; }"
    ":scope.zui-surface[data-instant] .zui-tooltip__arrow { animation: none; }"
    "@keyframes zui-tooltip-arrow-enter { from { opacity: 0; } to { opacity: 1; } }"
    "@keyframes zui-tooltip-arrow-exit { from { opacity: 1; } to { opacity: 0; } }"
}
