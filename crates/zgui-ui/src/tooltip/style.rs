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
    ":scope {
        width: fit-content;
        padding: calc(var(--zui-space-base) * 1.5) var(--zui-space-md);
        --zui-surface-border: none;
        --zui-surface-radius: var(--zui-radius-md);
        --zui-surface-shadow: none;
        --zui-surface-fill: var(--zui-color-foreground);
        --zui-surface-ink: var(--zui-color-background);
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
