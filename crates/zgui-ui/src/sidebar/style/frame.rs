//! The frame: the room the panel takes, the surface in it, the rail, and the page beside it.

use zgui::style;

style! { pub SidebarStyle =>
    // Two widths and nothing else. Everything below is laid out from them, so an application that
    // wants a wider panel writes one declaration rather than one override per part.
    ":scope {
        --zui-sidebar-width: 256px;
        --zui-sidebar-width-icon: 48px;
        display: flex;
        flex-direction: row;
        width: 100%;
        min-height: 100%;
    }"
    ":scope[data-side=\"right\"] { flex-direction: row-reverse; }"
    // The inset frame turns the page into the card, so what is behind everything has to be the
    // panel's own colour for the card to read as lifted off it.
    ":scope[data-variant=\"inset\"] { background-color: var(--zui-color-sidebar); }"

    // The room. A plain flow item whose width is the whole of the fold: the surface inside it is
    // placed rather than laid out, so it can slide off the edge instead of being squeezed flat
    // against it.
    ".zui-sidebar {
        position: relative;
        flex: 0 0 auto;
        width: var(--zui-sidebar-width);
        background-color: transparent;
        color: var(--zui-color-sidebar-foreground);
        transition: width var(--zui-motion-duration-slow) var(--zui-motion-ease-linear);
    }"
    ":scope[data-state=\"collapsed\"][data-collapsible=\"offcanvas\"] .zui-sidebar { width: 0px; }"
    ":scope[data-state=\"collapsed\"][data-collapsible=\"icon\"] .zui-sidebar {
        width: var(--zui-sidebar-width-icon);
    }"
    // A floating or inset panel holds its surface off the window by 8px on each side, so the room
    // it needs is that much wider than the icons in it.
    ":scope[data-state=\"collapsed\"][data-collapsible=\"icon\"][data-variant=\"floating\"] .zui-sidebar,
     :scope[data-state=\"collapsed\"][data-collapsible=\"icon\"][data-variant=\"inset\"] .zui-sidebar {
        width: calc(var(--zui-sidebar-width-icon) + 16px);
    }"

    // The surface's holder, which is what slides.
    ".zui-sidebar__container {
        position: absolute;
        top: 0;
        bottom: 0;
        left: 0;
        z-index: 10;
        display: flex;
        width: var(--zui-sidebar-width);
        transition:
            left var(--zui-motion-duration-slow) var(--zui-motion-ease-linear),
            right var(--zui-motion-duration-slow) var(--zui-motion-ease-linear),
            width var(--zui-motion-duration-slow) var(--zui-motion-ease-linear);
    }"
    ":scope[data-side=\"right\"] .zui-sidebar__container { left: auto; right: 0; }"
    ":scope[data-state=\"collapsed\"][data-collapsible=\"offcanvas\"] .zui-sidebar__container {
        left: calc(var(--zui-sidebar-width) * -1);
    }"
    ":scope[data-side=\"right\"][data-state=\"collapsed\"][data-collapsible=\"offcanvas\"] .zui-sidebar__container {
        left: auto;
        right: calc(var(--zui-sidebar-width) * -1);
    }"
    ":scope[data-variant=\"sidebar\"][data-side=\"left\"] .zui-sidebar__container {
        border-right: 1px solid var(--zui-color-sidebar-border);
    }"
    ":scope[data-variant=\"sidebar\"][data-side=\"right\"] .zui-sidebar__container {
        border-left: 1px solid var(--zui-color-sidebar-border);
    }"
    ":scope[data-variant=\"sidebar\"][data-state=\"collapsed\"][data-collapsible=\"icon\"] .zui-sidebar__container {
        width: var(--zui-sidebar-width-icon);
    }"
    ":scope[data-variant=\"floating\"] .zui-sidebar__container,
     :scope[data-variant=\"inset\"] .zui-sidebar__container { padding: 8px; }"
    // The extra 2px is the floating surface's own border, which the icons must not be pushed into.
    ":scope[data-variant=\"floating\"][data-state=\"collapsed\"][data-collapsible=\"icon\"] .zui-sidebar__container,
     :scope[data-variant=\"inset\"][data-state=\"collapsed\"][data-collapsible=\"icon\"] .zui-sidebar__container {
        width: calc(var(--zui-sidebar-width-icon) + 18px);
    }"

    ".zui-sidebar__inner {
        display: flex;
        flex-direction: column;
        width: 100%;
        height: 100%;
        background-color: var(--zui-color-sidebar);
    }"
    ":scope[data-variant=\"floating\"] .zui-sidebar__inner {
        border: 1px solid var(--zui-color-sidebar-border);
        border-radius: var(--zui-radius-lg);
        box-shadow: var(--zui-shadow-sm);
    }"

    // The rail straddles the panel's outer edge: half of its 16px is over the surface and half is
    // over the page, so the pointer finds it a little before the edge and a little after.
    ".zui-sidebar__rail {
        position: absolute;
        top: 0;
        bottom: 0;
        z-index: 20;
        display: flex;
        width: 16px;
        padding: 0;
        border: none;
        background-color: transparent;
        transform: translateX(-50%);
        transition:
            left var(--zui-motion-duration-slow) var(--zui-motion-ease-linear),
            right var(--zui-motion-duration-slow) var(--zui-motion-ease-linear),
            transform var(--zui-motion-duration-slow) var(--zui-motion-ease-linear),
            background-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ":scope[data-side=\"left\"] .zui-sidebar__rail { right: -16px; cursor: w-resize; }"
    ":scope[data-side=\"right\"] .zui-sidebar__rail { left: 0px; cursor: e-resize; }"
    ":scope[data-side=\"left\"][data-state=\"collapsed\"] .zui-sidebar__rail { cursor: e-resize; }"
    ":scope[data-side=\"right\"][data-state=\"collapsed\"] .zui-sidebar__rail { cursor: w-resize; }"
    // The line the rail shows on hover, which is the edge the panel would be dragged by.
    ".zui-sidebar__rail::after {
        content: \"\";
        position: absolute;
        top: 0;
        bottom: 0;
        left: 50%;
        width: 2px;
        background-color: transparent;
        transition: background-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ".zui-sidebar__rail:hover::after { background-color: var(--zui-color-sidebar-border); }"
    ".zui-sidebar__rail:focus-visible {
        outline: 2px solid var(--zui-color-sidebar-ring);
        outline-offset: -2px;
    }"
    // With the panel folded clean off the page there is no edge left to straddle, so the rail
    // stops straddling and becomes a strip of its own that lights up whole.
    ":scope[data-collapsible=\"offcanvas\"][data-state=\"collapsed\"] .zui-sidebar__rail {
        transform: translateX(0);
    }"
    ":scope[data-collapsible=\"offcanvas\"][data-state=\"collapsed\"] .zui-sidebar__rail::after {
        left: 100%;
    }"
    ":scope[data-collapsible=\"offcanvas\"][data-state=\"collapsed\"] .zui-sidebar__rail:hover {
        background-color: var(--zui-color-sidebar);
    }"
    ":scope[data-side=\"left\"][data-collapsible=\"offcanvas\"][data-state=\"collapsed\"] .zui-sidebar__rail {
        right: -8px;
    }"
    ":scope[data-side=\"right\"][data-collapsible=\"offcanvas\"][data-state=\"collapsed\"] .zui-sidebar__rail {
        left: -8px;
    }"

    ".zui-sidebar__inset {
        position: relative;
        display: flex;
        flex: 1 1 auto;
        flex-direction: column;
        width: 100%;
        min-width: 0;
        background-color: var(--zui-color-background);
    }"
    // The inset frame: the page is the card. Its margin on the panel's side is nothing, because
    // the panel's own padding already left the gap; folded away, the page takes that gap over.
    ":scope[data-variant=\"inset\"] .zui-sidebar__inset {
        margin: 8px;
        border-radius: var(--zui-radius-xl);
        box-shadow: var(--zui-shadow-sm);
    }"
    ":scope[data-variant=\"inset\"][data-side=\"left\"] .zui-sidebar__inset { margin-left: 0px; }"
    ":scope[data-variant=\"inset\"][data-side=\"right\"] .zui-sidebar__inset { margin-right: 0px; }"
    ":scope[data-variant=\"inset\"][data-side=\"left\"][data-state=\"collapsed\"] .zui-sidebar__inset {
        margin-left: 8px;
    }"
    ":scope[data-variant=\"inset\"][data-side=\"right\"][data-state=\"collapsed\"] .zui-sidebar__inset {
        margin-right: 8px;
    }"

    ".zui-sidebar__trigger {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
        width: 28px;
        height: 28px;
        padding: 0;
        border: none;
        border-radius: var(--zui-radius-md);
        background-color: transparent;
        color: var(--zui-color-foreground);
        transition:
            background-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
            color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ".zui-sidebar__trigger:hover {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"
    ".zui-sidebar__trigger:focus-visible {
        border-color: var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
        outline: none;
    }"
}
