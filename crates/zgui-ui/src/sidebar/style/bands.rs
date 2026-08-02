//! The bands stacked inside the panel, and the groups inside the scrolling one.

use zgui::style;

style! { pub SidebarBandStyle =>
    ".zui-sidebar__header, .zui-sidebar__footer {
        display: flex;
        flex-direction: column;
        gap: 8px;
        padding: 8px;
    }"

    // The only band that scrolls, and the only one that clips. Folded to icons there is no room
    // for a scrollbar beside a column of icons, so it holds what it has instead.
    ".zui-sidebar__content {
        display: flex;
        flex: 1 1 auto;
        flex-direction: column;
        min-height: 0;
        gap: 8px;
        overflow: auto;
    }"
    ".zui-sidebar-provider[data-collapsible=\"icon\"][data-state=\"collapsed\"] .zui-sidebar__content {
        overflow: hidden;
    }"

    // Inset from both edges and in the panel's own border colour, so it reads as part of the panel
    // rather than as the panel ending.
    ".zui-sidebar__separator {
        width: auto;
        margin: 0 8px;
        background-color: var(--zui-color-sidebar-border);
    }"

    // Shorter than an ordinary field and flat rather than lifted: it sits in a surface that is
    // already off the page, and a second lift on top of that reads as a second card.
    ".zui-sidebar__input {
        width: 100%;
        height: 32px;
        background-color: var(--zui-color-background);
        box-shadow: none;
    }"

    ".zui-sidebar__group {
        position: relative;
        display: flex;
        flex-direction: column;
        width: 100%;
        min-width: 0;
        padding: 8px;
    }"

    // Folding to icons does not fade the heading out and leave a hole: it pulls the heading up by
    // exactly its own height at the same time, so the entries below close the gap as it goes.
    ".zui-sidebar__group-label {
        display: flex;
        align-items: center;
        flex-shrink: 0;
        height: 32px;
        padding: 0 8px;
        border-radius: var(--zui-radius-md);
        color: color-mix(in oklab, var(--zui-color-sidebar-foreground) 70%, transparent);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-xs);
        line-height: var(--zui-type-leading-xs);
        font-weight: var(--zui-type-weight-medium);
        transition:
            margin-top var(--zui-motion-duration-slow) var(--zui-motion-ease-linear),
            opacity var(--zui-motion-duration-slow) var(--zui-motion-ease-linear);
    }"
    ".zui-sidebar__group-label:focus-visible {
        outline: 2px solid var(--zui-color-sidebar-ring);
        outline-offset: -2px;
    }"
    ".zui-sidebar-provider[data-collapsible=\"icon\"][data-state=\"collapsed\"] .zui-sidebar__group-label {
        margin-top: -32px;
        opacity: 0;
    }"

    ".zui-sidebar__group-action {
        position: absolute;
        top: 14px;
        right: 12px;
        display: flex;
        align-items: center;
        justify-content: center;
        width: 20px;
        height: 20px;
        padding: 0;
        border: none;
        border-radius: var(--zui-radius-md);
        background-color: transparent;
        color: var(--zui-color-sidebar-foreground);
        transition: transform var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ".zui-sidebar__group-action:hover {
        background-color: var(--zui-color-sidebar-accent);
        color: var(--zui-color-sidebar-accent-foreground);
    }"
    ".zui-sidebar__group-action:focus-visible {
        outline: 2px solid var(--zui-color-sidebar-ring);
        outline-offset: -2px;
    }"
    ".zui-sidebar-provider[data-collapsible=\"icon\"][data-state=\"collapsed\"] .zui-sidebar__group-action {
        display: none;
    }"
    ".zui-sidebar-provider[data-side=\"right\"] .zui-sidebar__group-action { right: auto; left: 12px; }"

    ".zui-sidebar__group-content {
        width: 100%;
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
    }"
}
