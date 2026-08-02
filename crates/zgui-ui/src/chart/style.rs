//! What a chart looks like, in tokens.

use zgui::style;

style! { pub ChartStyle =>
    // The box a chart is given. Sixteen by nine, because that is the shape a plot of a series
    // against time reads best in and the shape a caller would otherwise write out every time.
    ".zui-chart__container {
        display: flex;
        justify-content: center;
        aspect-ratio: 16 / 9;
        font-size: var(--zui-type-size-xs);
        line-height: var(--zui-type-leading-xs);
    }"
    ":scope {
        display: block;
        position: relative;
        color: var(--zui-color-foreground);
        font-size: var(--zui-type-size-xs);
        line-height: var(--zui-type-leading-xs);
    }"
    ":scope .zui-chart__plot {
        position: relative;
        width: var(--zui-chart-width);
        height: var(--zui-chart-height);
    }"
    // The axes cover the plot, because that is what they are.
    ":scope .zui-chart__axes {
        position: absolute;
        left: 0;
        top: 0;
        width: var(--zui-chart-width);
        height: var(--zui-chart-height);
    }"
    // A mark is placed at its own geometry rather than over the whole plot. That is what makes one
    // bar hoverable, hit-testable and outlinable on its own: a box is what a pointer answers from,
    // and marks that all covered the plot would be stacked with only the last one reachable.
    ":scope .zui-chart__mark {
        position: absolute;
        left: var(--zui-chart-mark-x, 0px);
        top: var(--zui-chart-mark-y, 0px);
        width: var(--zui-chart-mark-width, 0px);
        height: var(--zui-chart-mark-height, 0px);
    }"
    // Vector content takes its paint from `color` and from the two custom properties beside it,
    // because the SVG paint longhands are not properties this engine build has.
    ":scope .zui-chart__axes {
        color: transparent;
        --zgui-stroke: color-mix(in oklab, var(--zui-color-border) 50%, transparent);
        --zgui-stroke-width: 1px;
    }"
    ":scope .zui-chart__mark { color: var(--zui-chart-tone); }"
    ":scope .zui-chart__mark[data-shape=\"line\"] {
        color: transparent;
        --zgui-stroke: var(--zui-chart-tone);
        --zgui-stroke-width: 2px;
    }"
    ":scope .zui-chart__mark:hover { opacity: 0.8; }"
    ":scope .zui-chart__mark:focus-visible {
        outline: 2px solid var(--zui-color-ring);
        outline-offset: 1px;
    }"
    ":scope .zui-chart__label {
        position: absolute;
        color: var(--zui-color-muted-foreground);
    }"

    // ---- the key -------------------------------------------------------------------------------
    ":scope .zui-chart__legend {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        justify-content: center;
        gap: var(--zui-space-lg);
        padding-top: var(--zui-space-md);
    }"
    ":scope .zui-chart__legend[data-align=\"top\"] {
        padding-top: 0;
        padding-bottom: var(--zui-space-md);
    }"
    ":scope .zui-chart__key {
        display: flex;
        align-items: center;
        gap: calc(var(--zui-space-base) * 1.5);
    }"
    ":scope .zui-chart__swatch {
        width: 8px;
        height: 8px;
        flex: none;
        border-radius: 2px;
        background-color: var(--zui-chart-tone);
    }"

    // ---- the readout ---------------------------------------------------------------------------
    // Placed over the plot rather than in it, and deaf to the pointer: a card that answered the
    // pointer would put itself between the pointer and the mark that summoned it, and the two would
    // take it in turns to appear.
    ":scope .zui-chart__readout {
        position: absolute;
        left: var(--zui-chart-readout-x, 0px);
        top: var(--zui-chart-readout-y, 0px);
        transform: translate(-50%, -100%);
        pointer-events: none;
        z-index: 1;
    }"
    ".zui-chart__tooltip {
        display: grid;
        align-items: start;
        gap: calc(var(--zui-space-base) * 1.5);
        min-width: 128px;
        padding: calc(var(--zui-space-base) * 1.5) calc(var(--zui-space-base) * 2.5);
        border: 1px solid color-mix(in oklab, var(--zui-color-border) 50%, transparent);
        border-radius: var(--zui-radius-lg);
        background-color: var(--zui-color-background);
        color: var(--zui-color-foreground);
        font-size: var(--zui-type-size-xs);
        line-height: var(--zui-type-leading-xs);
        box-shadow: var(--zui-shadow-xl);
    }"
    ".zui-chart__tooltip-label { font-weight: var(--zui-type-weight-medium); }"
    ".zui-chart__tooltip-rows {
        display: grid;
        gap: calc(var(--zui-space-base) * 1.5);
    }"
    ".zui-chart__tooltip-row {
        display: flex;
        width: 100%;
        flex-wrap: wrap;
        align-items: stretch;
        gap: var(--zui-space-sm);
    }"
    ".zui-chart__tooltip-row[data-indicator=\"dot\"] { align-items: center; }"
    ".zui-chart__tooltip-swatch {
        flex: none;
        border-radius: 2px;
        background-color: var(--zui-chart-tone);
        border: 0 solid var(--zui-chart-tone);
    }"
    ".zui-chart__tooltip-row[data-indicator=\"dot\"] .zui-chart__tooltip-swatch {
        width: 10px;
        height: 10px;
    }"
    ".zui-chart__tooltip-row[data-indicator=\"line\"] .zui-chart__tooltip-swatch { width: 4px; }"
    ".zui-chart__tooltip-row[data-indicator=\"dashed\"] .zui-chart__tooltip-swatch {
        width: 0;
        border-width: 1.5px;
        border-style: dashed;
        background-color: transparent;
    }"
    ".zui-chart__tooltip-body {
        display: flex;
        flex: 1 1 0;
        align-items: center;
        justify-content: space-between;
        line-height: 1;
        gap: var(--zui-space-sm);
    }"
    ".zui-chart__tooltip-name { color: var(--zui-color-muted-foreground); }"
    ".zui-chart__tooltip-value {
        font-family: var(--zui-type-family-mono);
        font-weight: var(--zui-type-weight-medium);
        color: var(--zui-color-foreground);
        font-variant-numeric: tabular-nums;
    }"
}
