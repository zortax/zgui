//! What a disclosure looks like, in tokens.

use zgui::style;

style! { pub CollapsibleStyle =>
    ":scope { display: flex; flex-direction: column; }"
    ".zui-collapsible__trigger {
        display: flex;
        flex-direction: row;
        align-items: center;
        justify-content: space-between;
        gap: var(--zui-space-sm);
        padding: var(--zui-space-sm) 0;
        border: none;
        border-radius: var(--zui-radius-md);
        background-color: transparent;
        color: var(--zui-color-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-medium);
        line-height: var(--zui-type-leading-sm);
        text-align: left;
    }"
    ".zui-collapsible__trigger:focus-visible {
        outline: none;
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
    ".zui-collapsible__trigger:disabled { opacity: 0.5; pointer-events: none; }"
    // Clipped, no taller than nothing, and animating towards whatever the measurement said. The
    // fallback keeps a section that has not been measured yet — the very first frame — from
    // collapsing an open disclosure to a sliver.
    ".zui-collapsible__content {
        display: block;
        overflow: hidden;
        height: 0;
        transition: height var(--zui-motion-duration-slow) var(--zui-motion-ease-out);
    }"
    ".zui-collapsible__content[data-state=\"open\"] {
        height: var(--zui-collapsible-height, auto);
    }"
    ".zui-collapsible__measure { display: flex; flex-direction: column; }"
}
