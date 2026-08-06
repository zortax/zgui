//! What a menu looks like, in tokens.

use zgui::style;

style! { pub MenuStyle =>
    // A menu is the shared surface at a list's proportions: narrow padding all round, and no gap
    // at all between the rows. They are a continuous column that the highlight travels down, and a
    // gap between them would make it flicker off and on as the pointer crossed each seam.
    ":scope {
        min-width: 128px;
        padding: var(--zui-space-xs);
        gap: 0;
        overflow-x: hidden;
        overflow-y: auto;
    }"
    // A submenu sits over the menu that opened it rather than over the page, so it is lifted one
    // rung further off — and it does not scroll, because a submenu long enough to need scrolling
    // is a menu that should have been one.
    ".zui-menu--sub { --zui-surface-shadow: var(--zui-shadow-lg); overflow: hidden; }"

    // The list and the typeahead both build an element that is a behaviour rather than a box.
    ".zui-menu__list, .zui-menu__keys { display: contents; }"

    // A row. `position: relative` is not decoration: a tick is placed against the row rather than
    // laid out in it, so that a row with one and a row without indent their labels to the same
    // column.
    ".zui-menu__item {
        position: relative;
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: var(--zui-space-sm);
        padding: var(--zui-space-base) var(--zui-space-sm);
        border: none;
        border-radius: var(--zui-radius-sm);
        background-color: transparent;
        outline: none;
        text-align: left;
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        color: var(--zui-color-popover-foreground);
    }"
    // The highlight is the focus, and the focus is the engine's. A menu that kept a `highlighted`
    // signal beside it would be a menu that disagrees with itself on the frame the pointer leaves.
    ".zui-menu__item:focus, .zui-menu__item:hover {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"
    ".zui-menu__item:disabled { opacity: 0.5; pointer-events: none; }"
    // A row that opened a submenu stays lit while that submenu is up, so the trail from the menu
    // to the surface beside it is visible rather than remembered.
    ".zui-menu__item[data-state=\"open\"] {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"

    // A row indented to line up with the rows that carry a tick, although it carries none. Without
    // it, a run of choices with one plain command among them steps in and out.
    ".zui-menu__item--inset, .zui-menu__label--inset { padding-left: var(--zui-space-2xl); }"

    // A destructive row keeps the highlight's *shape* and changes its colour: a tenth of the
    // destructive colour under it, and the full strength on the text and on whatever symbol it
    // carries. A row that turned grey would read as disabled rather than as dangerous.
    ".zui-menu__item--destructive { color: var(--zui-color-destructive); }"
    ".zui-menu__item--destructive:focus, .zui-menu__item--destructive:hover {
        background-color: color-mix(in oklab, var(--zui-color-destructive) 10%, transparent);
        color: var(--zui-color-destructive);
    }"
    ".zui-menu__item--destructive .zui-icon { color: var(--zui-color-destructive); }"

    // A symbol beside a label is subordinate to it, so it is drawn at the muted weight until the
    // row lights up — at which point it takes the row's colour along with everything else on it.
    ".zui-menu__item .zui-icon { color: var(--zui-color-muted-foreground); }"
    ".zui-menu__item:focus .zui-icon, .zui-menu__item:hover .zui-icon { color: inherit; }"

    // A tick or a bullet is placed in the gutter the row's own left padding opened for it, so a
    // label sits in the same column whether or not its row is chosen.
    ".zui-menu__item--check {
        padding-left: var(--zui-space-2xl);
        padding-right: var(--zui-space-sm);
    }"
    ".zui-menu__indicator {
        position: absolute;
        left: var(--zui-space-sm);
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 14px;
        height: 14px;
        pointer-events: none;
    }"
    // The bullet of a chosen radio row is a disc half the size of a tick, in the row's own colour
    // rather than the muted one every other symbol takes.
    ".zui-menu__indicator .zui-icon { color: inherit; }"
    ".zui-menu__indicator--dot .zui-icon { width: 8px; height: 8px; }"

    ".zui-menu__label {
        padding: var(--zui-space-base) var(--zui-space-sm);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        font-weight: var(--zui-type-weight-medium);
    }"
    // The rule between two runs of rows reaches past the menu's padding to both of its edges,
    // which is what makes it a division of the menu rather than a line drawn inside one.
    ".zui-menu__separator {
        height: 1px;
        margin: var(--zui-space-xs) calc(var(--zui-space-xs) * -1);
        background-color: var(--zui-color-border);
    }"
    ".zui-menu__shortcut {
        margin-left: auto;
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-xs);
        letter-spacing: var(--zui-type-tracking-widest);
        color: var(--zui-color-muted-foreground);
    }"
    ".zui-menu__chevron { margin-left: auto; }"
}
