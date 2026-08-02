//! What a navigation menu looks like, in tokens.

use zgui::style;

style! { pub NavigationMenuStyle =>
    ":scope {
        position: relative;
        display: flex;
        flex: 1 1 auto;
        max-width: max-content;
        align-items: center;
        justify-content: center;
    }"
    ".zui-navigation-menu__list {
        display: flex;
        flex: 1 1 auto;
        flex-direction: row;
        align-items: center;
        justify-content: center;
        gap: var(--zui-space-xs);
        list-style: none;
    }"
    ".zui-navigation-menu__item { position: relative; display: inline-flex; }"

    // What sits on the bar: a section's trigger, and a link written straight into a section that
    // has no panel. The link inside a *panel* is a different shape entirely, further down.
    ".zui-navigation-menu__trigger,
     .zui-navigation-menu__item > .zui-navigation-menu__link {
        display: inline-flex;
        flex-direction: row;
        width: max-content;
        height: calc(var(--zui-space-base) * 9);
        align-items: center;
        justify-content: center;
        padding: var(--zui-space-sm) var(--zui-space-lg);
        border: none;
        border-radius: var(--zui-radius-md);
        background-color: var(--zui-color-background);
        color: var(--zui-color-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-medium);
        line-height: var(--zui-type-leading-sm);
        transition: color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    background-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    box-shadow var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ".zui-navigation-menu__trigger:hover,
     .zui-navigation-menu__item > .zui-navigation-menu__link:hover {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"
    // An open section is held down at half strength, and goes to full under the pointer — so the
    // one that is open still answers a hover rather than looking stuck.
    ".zui-navigation-menu__trigger[data-state=\"open\"] {
        background-color: color-mix(in oklab, var(--zui-color-accent) 50%, transparent);
        color: var(--zui-color-accent-foreground);
    }"
    ".zui-navigation-menu__trigger[data-state=\"open\"]:hover {
        background-color: var(--zui-color-accent);
    }"
    ".zui-navigation-menu__link[data-active=\"true\"] {
        background-color: color-mix(in oklab, var(--zui-color-accent) 50%, transparent);
        color: var(--zui-color-accent-foreground);
    }"
    ".zui-navigation-menu__link[data-active=\"true\"]:hover {
        background-color: var(--zui-color-accent);
    }"
    ".zui-navigation-menu__trigger:focus-visible,
     .zui-navigation-menu__link:focus-visible {
        outline: 1px solid var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
    ".zui-navigation-menu__trigger:disabled { opacity: 0.5; pointer-events: none; }"

    // Slower than the panel it belongs to, because it is the one part of the gesture a reader
    // watches rather than reads: half a turn, ending as the panel finishes arriving.
    ".zui-navigation-menu__chevron {
        --zui-icon-md: var(--zui-space-md);
        position: relative;
        top: 1px;
        margin-left: var(--zui-space-xs);
        transition: transform var(--zui-motion-duration-slower) var(--zui-motion-ease-standard);
        pointer-events: none;
    }"
    ".zui-navigation-menu__trigger[data-state=\"open\"] .zui-navigation-menu__chevron {
        transform: rotate(180deg);
    }"

    ".zui-navigation-menu__content {
        display: flex;
        flex-direction: column;
        min-width: 200px;
        overflow: hidden;
        border: 1px solid var(--zui-color-border);
        border-radius: var(--zui-radius-md);
        background-color: var(--zui-color-popover);
        color: var(--zui-color-popover-foreground);
        box-shadow: var(--zui-shadow-sm);
        opacity: 1;
        transform: scale(1);
        transform-origin: top center;
        transition: opacity var(--zui-motion-duration-normal) ease,
                    transform var(--zui-motion-duration-normal) ease;
    }"
    // The dismiss layer is the panel's inner box rather than an invisible behaviour, and the
    // panel's padding is *its* padding on purpose: whether a press is inside the layer is decided
    // by tree containment, so padding left on the panel would be past the layer — and a press on
    // the panel's own edge would close the menu from inside it.
    ".zui-navigation-menu__dismiss {
        display: flex;
        flex-direction: column;
        gap: var(--zui-space-xs);
        padding: var(--zui-space-sm) calc(var(--zui-space-base) * 2.5) var(--zui-space-sm)
                 var(--zui-space-sm);
    }"

    // Arrives from slightly under its own size and leaves from slightly over it, which is what
    // makes a panel look like it came out of the trigger rather than fading in over the page.
    ".zui-navigation-menu__content[data-state=\"closed\"] {
        opacity: 0;
        transform: scale(0.9);
        pointer-events: none;
    }"

    // A link inside a panel is a block of writing, not a tab on a bar.
    ".zui-navigation-menu__content .zui-navigation-menu__link {
        display: flex;
        flex-direction: column;
        gap: var(--zui-space-xs);
        width: 100%;
        height: auto;
        align-items: stretch;
        justify-content: flex-start;
        padding: var(--zui-space-sm);
        border-radius: var(--zui-radius-sm);
        background-color: transparent;
        text-align: left;
        font-weight: var(--zui-type-weight-normal);
    }"

    // The arrow that points from the bar at the open panel: a square turned on its corner, with
    // its bottom half clipped away by the strip it sits in, which is what leaves a triangle.
    ".zui-navigation-menu__indicator {
        position: absolute;
        top: 100%;
        left: 0;
        right: 0;
        z-index: 1;
        display: flex;
        height: calc(var(--zui-space-base) * 1.5);
        align-items: flex-end;
        justify-content: center;
        overflow: hidden;
        opacity: 1;
        transition: opacity var(--zui-motion-duration-normal) ease;
        pointer-events: none;
    }"
    ".zui-navigation-menu__indicator[data-state=\"hidden\"] { opacity: 0; }"
    ".zui-navigation-menu__indicator-arrow {
        position: relative;
        top: 60%;
        width: var(--zui-space-sm);
        height: var(--zui-space-sm);
        border-top-left-radius: var(--zui-radius-sm);
        background-color: var(--zui-color-border);
        box-shadow: var(--zui-shadow-md);
        transform: rotate(45deg);
    }"

    // The box that places the portalled panel, which is a direct child of an overlay band. Two
    // things it needs from being there, and it is the only positioner in this library that needs
    // either, because it is the only one whose panel stays mounted while it is shut.
    //
    // It takes no pointer events. Everything on an overlay band does by default, because that is
    // what a menu and a dialog need; a shut panel's positioner is an invisible rectangle sitting
    // over the page under every trigger in the bar, and one that accepted a press would swallow
    // every click that landed under one. The panel itself takes them back while it is open.
    //
    // It stacks by the depth the surface it was opened from published, exactly as every other
    // floating surface in this library does, so a navigation menu inside a dialog is drawn over
    // the dialog rather than under it.
    ".zui-navigation-menu__positioner {
        pointer-events: none;
        z-index: var(--zui-overlay-depth, 0);
    }"
    ".zui-navigation-menu__content { pointer-events: auto; }"
}
