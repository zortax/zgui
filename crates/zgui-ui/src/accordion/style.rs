//! What an accordion looks like, in tokens.

use zgui::style;

style! { pub AccordionStyle =>
    ":scope { display: flex; flex-direction: column; }"
    ".zui-accordion__item {
        display: flex;
        flex-direction: column;
        border-bottom: 1px solid var(--zui-color-border);
    }"
    // The last rule is the one whatever holds the accordion already draws. Two lines a pixel apart
    // read as one thick one.
    ".zui-accordion__item:last-child { border-bottom: none; }"
    ".zui-accordion__header { display: flex; flex-direction: row; }"

    // Aligned to the top rather than the middle, so a two-line question keeps its chevron beside
    // the first line instead of floating it into the gap between them.
    ".zui-accordion__trigger {
        display: flex;
        flex: 1 1 auto;
        flex-direction: row;
        align-items: flex-start;
        justify-content: space-between;
        gap: var(--zui-space-lg);
        padding: var(--zui-space-lg) 0;
        border: none;
        border-radius: var(--zui-radius-md);
        background-color: transparent;
        color: var(--zui-color-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-medium);
        line-height: var(--zui-type-leading-sm);
        text-align: left;
        transition: color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    box-shadow var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ".zui-accordion__trigger:hover { text-decoration-line: underline; }"
    ".zui-accordion__trigger:focus-visible {
        outline: none;
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
    ".zui-accordion__trigger:disabled { opacity: 0.5; pointer-events: none; }"

    // The turn is paced with the slide it announces rather than with the colour changes above it.
    ".zui-accordion__chevron {
        flex: none;
        color: var(--zui-color-muted-foreground);
        transform: translateY(2px);
        transition: transform var(--zui-motion-duration-slow) var(--zui-motion-ease-standard);
        pointer-events: none;
    }"
    ".zui-accordion__trigger[data-state=\"open\"] .zui-accordion__chevron {
        transform: translateY(2px) rotate(180deg);
    }"

    ".zui-accordion__content {
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
    }"
    ".zui-accordion__content .zui-collapsible__measure { padding-bottom: var(--zui-space-lg); }"
}
