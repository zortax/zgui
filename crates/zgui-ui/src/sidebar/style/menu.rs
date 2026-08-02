//! The list of places the panel holds, and everything that hangs off an entry in it.

use zgui::style;

style! { pub SidebarMenuStyle =>
    ".zui-sidebar__menu {
        display: flex;
        flex-direction: column;
        width: 100%;
        min-width: 0;
        gap: 4px;
    }"
    // An entry stacks: the button, and under it whatever nested list the caller put beside the
    // button. In shadcn the item is a plain block and stacking is what blocks do; here everything
    // is flex, so the direction has to say it — a row would seat the sublist to the button's right,
    // squeezed into whatever width the button left over.
    ".zui-sidebar__menu-item { position: relative; display: flex; flex-direction: column; }"

    // The entry. Folding to icons squares it to 32px and clips everything past the icon, which is
    // why the width, the height and the padding are what the transition names: the label is not
    // faded out, it is simply no longer in the box.
    ".zui-sidebar__menu-button {
        display: flex;
        flex: 1 1 auto;
        align-items: center;
        gap: 8px;
        width: 100%;
        padding: 8px;
        border: none;
        border-radius: var(--zui-radius-md);
        background-color: transparent;
        color: inherit;
        font-family: var(--zui-type-family-sans);
        text-align: left;
        overflow: hidden;
        transition:
            width var(--zui-motion-duration-slow) var(--zui-motion-ease-linear),
            height var(--zui-motion-duration-slow) var(--zui-motion-ease-linear),
            padding var(--zui-motion-duration-slow) var(--zui-motion-ease-linear),
            background-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
            color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ".zui-sidebar__menu-button[data-size=\"default\"] {
        height: 32px;
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
    }"
    ".zui-sidebar__menu-button[data-size=\"sm\"] {
        height: 28px;
        font-size: var(--zui-type-size-xs);
        line-height: var(--zui-type-leading-xs);
    }"
    ".zui-sidebar__menu-button[data-size=\"lg\"] {
        height: 48px;
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
    }"
    ".zui-sidebar__menu-button:hover {
        background-color: var(--zui-color-sidebar-accent);
        color: var(--zui-color-sidebar-accent-foreground);
    }"
    ".zui-sidebar__menu-button:active {
        background-color: var(--zui-color-sidebar-accent);
        color: var(--zui-color-sidebar-accent-foreground);
    }"
    ".zui-sidebar__menu-button[data-active=\"true\"] {
        background-color: var(--zui-color-sidebar-accent);
        color: var(--zui-color-sidebar-accent-foreground);
        font-weight: var(--zui-type-weight-medium);
    }"
    ".zui-sidebar__menu-button[data-state=\"open\"]:hover {
        background-color: var(--zui-color-sidebar-accent);
        color: var(--zui-color-sidebar-accent-foreground);
    }"
    ".zui-sidebar__menu-button:focus-visible {
        outline: 2px solid var(--zui-color-sidebar-ring);
        outline-offset: -2px;
    }"
    ".zui-sidebar__menu-button:disabled { opacity: 0.5; pointer-events: none; }"
    // The outlined entry is drawn on the page's own surface with a ring rather than a border, so
    // that turning it on does not move the label by a pixel.
    ".zui-sidebar__menu-button[data-variant=\"outline\"] {
        background-color: var(--zui-color-background);
        box-shadow: 0 0 0 1px var(--zui-color-sidebar-border);
    }"
    ".zui-sidebar__menu-button[data-variant=\"outline\"]:hover {
        background-color: var(--zui-color-sidebar-accent);
        color: var(--zui-color-sidebar-accent-foreground);
        box-shadow: 0 0 0 1px var(--zui-color-sidebar-accent);
    }"
    // Room on the right for whatever the entry carries, so a long label ends in an ellipsis before
    // it reaches the badge rather than under it.
    ".zui-sidebar__menu-item[data-action=\"true\"] .zui-sidebar__menu-button { padding-right: 32px; }"
    ".zui-sidebar__menu-label {
        flex: 1 1 auto;
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }"
    ".zui-sidebar-provider[data-collapsible=\"icon\"][data-state=\"collapsed\"] .zui-sidebar__menu-button {
        width: 32px;
        height: 32px;
        padding: 8px;
    }"
    ".zui-sidebar-provider[data-collapsible=\"icon\"][data-state=\"collapsed\"] .zui-sidebar__menu-button[data-size=\"lg\"] {
        padding: 0px;
    }"
    // A tooltip wrapper is a behaviour, not a box: it must not narrow the entry it is around.
    ".zui-sidebar__menu-tip { display: flex; width: 100%; }"

    ".zui-sidebar__menu-action {
        position: absolute;
        right: 4px;
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
        transition:
            transform var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
            opacity var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ".zui-sidebar__menu-action[data-size=\"sm\"] { top: 4px; }"
    ".zui-sidebar__menu-action[data-size=\"default\"] { top: 6px; }"
    ".zui-sidebar__menu-action[data-size=\"lg\"] { top: 10px; }"
    ".zui-sidebar__menu-action:hover {
        background-color: var(--zui-color-sidebar-accent);
        color: var(--zui-color-sidebar-accent-foreground);
    }"
    ".zui-sidebar__menu-action:focus-visible {
        outline: 2px solid var(--zui-color-sidebar-ring);
        outline-offset: -2px;
    }"
    ".zui-sidebar__menu-item:hover .zui-sidebar__menu-action,
     .zui-sidebar__menu-item[data-active=\"true\"] .zui-sidebar__menu-action {
        color: var(--zui-color-sidebar-accent-foreground);
    }"
    ".zui-sidebar__menu-action[data-hover-only=\"true\"] { opacity: 0; }"
    ".zui-sidebar__menu-item:hover .zui-sidebar__menu-action[data-hover-only=\"true\"],
     .zui-sidebar__menu-item:focus-within .zui-sidebar__menu-action[data-hover-only=\"true\"],
     .zui-sidebar__menu-action[data-hover-only=\"true\"][data-state=\"open\"] { opacity: 1; }"
    ".zui-sidebar-provider[data-collapsible=\"icon\"][data-state=\"collapsed\"] .zui-sidebar__menu-action {
        display: none;
    }"

    // A count, not a control: it is never pointed at and never selected, because reaching for it
    // is always a miss for the entry underneath.
    ".zui-sidebar__menu-badge {
        position: absolute;
        right: 4px;
        display: flex;
        align-items: center;
        justify-content: center;
        height: 20px;
        min-width: 20px;
        padding: 0 4px;
        border-radius: var(--zui-radius-md);
        color: var(--zui-color-sidebar-foreground);
        font-size: var(--zui-type-size-xs);
        line-height: var(--zui-type-leading-xs);
        font-weight: var(--zui-type-weight-medium);
        font-variant-numeric: tabular-nums;
        pointer-events: none;
        user-select: none;
    }"
    ".zui-sidebar__menu-badge[data-size=\"sm\"] { top: 4px; }"
    ".zui-sidebar__menu-badge[data-size=\"default\"] { top: 6px; }"
    ".zui-sidebar__menu-badge[data-size=\"lg\"] { top: 10px; }"
    ".zui-sidebar__menu-item:hover .zui-sidebar__menu-badge,
     .zui-sidebar__menu-item[data-active=\"true\"] .zui-sidebar__menu-badge {
        color: var(--zui-color-sidebar-accent-foreground);
    }"
    ".zui-sidebar-provider[data-collapsible=\"icon\"][data-state=\"collapsed\"] .zui-sidebar__menu-badge {
        display: none;
    }"

    ".zui-sidebar__menu-skeleton {
        display: flex;
        align-items: center;
        gap: 8px;
        height: 32px;
        padding: 0 8px;
        border-radius: var(--zui-radius-md);
    }"
    ".zui-sidebar__menu-skeleton-icon {
        width: 16px;
        height: 16px;
        border-radius: var(--zui-radius-md);
    }"
    ".zui-sidebar__menu-skeleton-text {
        flex: 1 1 auto;
        height: 16px;
        max-width: var(--zui-sidebar-skeleton-width);
    }"

    // The nested list hangs off a rule down its left, and the rule is where the indent comes from.
    ".zui-sidebar__menu-sub {
        display: flex;
        flex-direction: column;
        min-width: 0;
        gap: 4px;
        margin: 0 14px;
        padding: 2px 10px;
        border-left: 1px solid var(--zui-color-sidebar-border);
        transform: translateX(1px);
    }"
    ".zui-sidebar-provider[data-collapsible=\"icon\"][data-state=\"collapsed\"] .zui-sidebar__menu-sub {
        display: none;
    }"
    ".zui-sidebar__menu-sub-item { position: relative; display: flex; }"
    ".zui-sidebar__menu-sub-button {
        display: flex;
        flex: 1 1 auto;
        align-items: center;
        gap: 8px;
        height: 28px;
        min-width: 0;
        padding: 0 8px;
        border: none;
        border-radius: var(--zui-radius-md);
        background-color: transparent;
        color: var(--zui-color-sidebar-foreground);
        font-family: var(--zui-type-family-sans);
        text-align: left;
        white-space: nowrap;
        overflow: hidden;
        transform: translateX(-1px);
        transition:
            background-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
            color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ".zui-sidebar__menu-sub-button[data-size=\"sm\"] {
        font-size: var(--zui-type-size-xs);
        line-height: var(--zui-type-leading-xs);
    }"
    ".zui-sidebar__menu-sub-button[data-size=\"md\"] {
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
    }"
    ".zui-sidebar__menu-sub-button .zui-icon { color: var(--zui-color-sidebar-accent-foreground); }"
    ".zui-sidebar__menu-sub-button:hover,
     .zui-sidebar__menu-sub-button:active,
     .zui-sidebar__menu-sub-button[data-active=\"true\"] {
        background-color: var(--zui-color-sidebar-accent);
        color: var(--zui-color-sidebar-accent-foreground);
    }"
    ".zui-sidebar__menu-sub-button:focus-visible {
        outline: 2px solid var(--zui-color-sidebar-ring);
        outline-offset: -2px;
    }"
    ".zui-sidebar__menu-sub-button:disabled { opacity: 0.5; pointer-events: none; }"
}
