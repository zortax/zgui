//! What a menubar looks like, in tokens.

use zgui::style;

style! { pub MenubarStyle =>
    // The bar itself: a bordered strip the height of one control, with its titles laid along it.
    // The shadow is the shallowest rung there is — a menubar sits *on* the page rather than over
    // it, and the only thing the shadow has to do is lift the strip off what is under it.
    ":scope {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: var(--zui-space-xs);
        height: 34px;
        padding: var(--zui-space-xs);
        border: 1px solid var(--zui-color-border);
        border-radius: var(--zui-radius-md);
        background-color: var(--zui-color-background);
        box-shadow: var(--zui-shadow-xs);
    }"
    ".zui-menubar__menu { display: inline-flex; }"
    // A behaviour rather than a box: the items are laid out by the surface, and a reader meets
    // them as the menu's own children.
    ".zui-menubar__arrows { display: contents; }"

    // A title on the bar. It lights up the same way whether it was reached with the pointer, with
    // the keyboard, or by its menu being open — one appearance for one state, so that walking the
    // bar with the arrow keys looks exactly like dragging along it.
    //
    // `:focus-visible` rather than `:focus`, and that is the difference between a bar and a bar
    // with a title stuck down on it. A menu hands the keyboard back to the title it came from when
    // it closes, so a menu dismissed by a press somewhere else in the window leaves the title
    // focused — and a title lit by plain `:focus` then reads as chosen, over a menu that is gone,
    // until something else is clicked. Reached with the keyboard it still lights, because that is
    // exactly when the focus is visible.
    ".zui-menubar__trigger {
        display: flex;
        flex-direction: row;
        align-items: center;
        padding: var(--zui-space-xs) var(--zui-space-sm);
        border: none;
        border-radius: var(--zui-radius-sm);
        background-color: transparent;
        color: var(--zui-color-foreground);
        outline: none;
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        font-weight: var(--zui-type-weight-medium);
    }"
    ".zui-menubar__trigger:hover,
     .zui-menubar__trigger:focus-visible,
     .zui-menubar__trigger[data-state=\"open\"] {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"
    ".zui-menubar__trigger:focus-visible {
        outline: 2px solid var(--zui-color-ring);
        outline-offset: 2px;
    }"

    // The menu that drops out of a title. Wider than one opened from a button, because what hangs
    // off a menubar is a whole heading's worth of commands rather than a row's.
    ".zui-menubar__content {
        display: flex;
        flex-direction: column;
        min-width: 192px;
        padding: var(--zui-space-xs);
        border: 1px solid var(--zui-color-border);
        border-radius: var(--zui-radius-md);
        background-color: var(--zui-color-popover);
        color: var(--zui-color-popover-foreground);
        box-shadow: var(--zui-shadow-md);
        overflow: hidden;
    }"
    // It always drops out of the bar, so it always comes in from above: eight pixels up, at
    // nineteen twentieths and transparent, settling over the same seventh of a second every other
    // surface in this library takes. It leaves with no animation at all, which is a decision
    // rather than an omission: walking the bar swaps menus, and a leaving menu that faded would
    // stand beside the arriving one for the whole fade — two open menus on one bar, which is a
    // picture the bar's own rule (one open at a time) says cannot happen. With nothing running,
    // the presence around the surface unmounts it on the very next frame, so a swap reads as the
    // menu *moving* rather than as one menu dying while another is born. shadcn's menubar makes
    // the same choice for the same reason.
    ".zui-menubar__content[data-state=\"open\"] {
        animation: zui-menubar-in var(--zui-motion-duration-normal) ease both;
    }"
    // No pointer for the one frame the closed surface is still mounted, so nothing can land a
    // press on a menu that has already been dismissed.
    ".zui-menubar__content[data-state=\"closed\"] {
        pointer-events: none;
    }"
    "@keyframes zui-menubar-in {
        from { opacity: 0; transform: translateY(-8px) scale(0.95); }
        to { opacity: 1; transform: translateY(0px) scale(1); }
    }"

    ".zui-menubar__item {
        position: relative;
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: var(--zui-space-sm);
        padding: var(--zui-space-base) var(--zui-space-sm);
        border: none;
        border-radius: var(--zui-radius-sm);
        background-color: transparent;
        color: inherit;
        outline: none;
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        text-align: left;
    }"
    ".zui-menubar__item:hover, .zui-menubar__item:focus {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"
    ".zui-menubar__item:disabled { opacity: 0.5; pointer-events: none; }"
    ".zui-menubar__item-label { flex: 1 1 auto; }"
    // A tick or a bullet is placed in the gutter the row's own left padding opens for it, so every
    // label on the menu starts in the same column whether its row carries a mark or not.
    ".zui-menubar__item--check {
        padding-left: var(--zui-space-2xl);
        padding-right: var(--zui-space-sm);
    }"
    ".zui-menubar__indicator {
        position: absolute;
        left: var(--zui-space-sm);
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 14px;
        height: 14px;
        pointer-events: none;
    }"
    ".zui-menubar__indicator--dot .zui-icon { width: 8px; height: 8px; }"
    ".zui-menubar__shortcut {
        margin-left: auto;
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-xs);
        letter-spacing: var(--zui-type-tracking-widest);
        color: var(--zui-color-muted-foreground);
    }"
    // The rule reaches past the menu's padding to both edges, so it divides the menu rather than
    // being a line drawn inside one.
    ".zui-menubar__separator {
        height: 1px;
        margin: var(--zui-space-xs) calc(var(--zui-space-xs) * -1);
        background-color: var(--zui-color-border);
    }"
    ".zui-menubar__label {
        padding: var(--zui-space-base) var(--zui-space-sm);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        font-weight: var(--zui-type-weight-medium);
    }"
}
