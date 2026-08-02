//! What a data table looks like, in tokens.

use zgui::style;

style! { pub DataTableStyle =>
    ":scope {
        display: flex;
        flex-direction: column;
        color: var(--zui-color-foreground);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
    }"
    // The bands above and below the table are spaced by their own padding rather than by a gap on
    // the column, so a table with no toolbar and no pager has no stray space around it.
    ":scope .zui-data-table__toolbar {
        display: flex;
        align-items: center;
        gap: var(--zui-space-sm);
        padding: var(--zui-space-lg) 0;
    }"
    // The search box is a field rather than a bar: a filter as wide as the table is one whose text
    // has nothing to be measured against.
    ":scope .zui-data-table__search { max-width: 384px; }"
    ":scope .zui-data-table__count {
        color: var(--zui-color-muted-foreground);
        margin-left: auto;
    }"
    // The table itself is the scroll container, so the header cells can stick to its top edge and
    // the body scrolls under them. A scroll box around the body instead would be one grid item
    // holding every row, and the columns would stop lining up with the header.
    ":scope .zui-data-table__grid {
        overflow-y: auto;
        overflow-x: hidden;
        border: 1px solid var(--zui-color-border);
        border-radius: var(--zui-radius-md);
        max-height: var(--zui-data-table-height, none);
    }"
    // The two spacers a virtualised body puts in place of the rows it did not build. Grid items
    // spanning every column, so the scrollbar is the length of the whole table.
    ":scope .zui-data-table__spacer {
        grid-column: 1 / -1;
        padding: 0;
        border: none;
        height: var(--zui-data-table-spacer, 0px);
    }"
    // A sortable heading is a button the width of its cell, drawn as nothing until it is under the
    // pointer: the column name is the label, and a heading that looked like a button would make a
    // table of eight columns look like a row of eight buttons.
    ":scope .zui-data-table__sort {
        display: inline-flex;
        align-items: center;
        gap: calc(var(--zui-space-base) * 1.5);
        height: 32px;
        margin-left: calc(0px - var(--zui-space-sm));
        padding: 0 var(--zui-space-sm);
        background: transparent;
        border: none;
        border-radius: var(--zui-radius-md);
        color: inherit;
        font: inherit;
        font-weight: var(--zui-type-weight-medium);
        flex: 1;
        min-width: 0;
        text-align: inherit;
        transition: background-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ":scope .zui-data-table__sort:hover {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"
    ":scope .zui-data-table__sort:focus-visible {
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
    ":scope .zui-data-table__arrow { color: var(--zui-color-muted-foreground); flex: none; }"
    ":scope .zui-data-table__grip {
        width: 6px;
        align-self: stretch;
        flex: none;
        margin-right: calc(0px - var(--zui-space-sm));
        cursor: col-resize;
        background: transparent;
    }"
    ":scope .zui-data-table__grip:hover { background-color: var(--zui-color-border); }"
    ":scope .zui-data-table__grip:focus-visible {
        outline: 2px solid var(--zui-color-ring);
        outline-offset: -1px;
    }"
    ":scope .zui-data-table__pager {
        display: flex;
        align-items: center;
        gap: var(--zui-space-sm);
        justify-content: flex-end;
        padding: var(--zui-space-lg) 0;
    }"
    // The pager's own buttons. Written here rather than borrowed from another component's sheet:
    // a sheet is installed by the component that owns it, so a table styled out of the button's
    // rules would be unstyled on every page that has no button on it. They are the outlined button
    // at its small size, declaration for declaration.
    ":scope .zui-data-table__step {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: calc(var(--zui-space-base) * 1.5);
        height: 32px;
        flex: none;
        padding: 0 var(--zui-space-md);
        border: 1px solid var(--zui-color-border);
        border-radius: var(--zui-radius-md);
        background-color: var(--zui-color-background);
        box-shadow: var(--zui-shadow-xs);
        color: var(--zui-color-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-medium);
        white-space: nowrap;
        transition: background-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    border-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ":scope .zui-data-table__step:hover {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"
    ":scope .zui-data-table__step:focus-visible {
        border-color: var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
    ":scope .zui-data-table__step[data-disabled=\"true\"] {
        opacity: 0.5;
        pointer-events: none;
    }"
    ":scope .zui-data-table__page {
        color: var(--zui-color-muted-foreground);
        margin-right: auto;
    }"
    // Deep enough to read as an empty table rather than as a missing row.
    ":scope .zui-data-table__empty {
        grid-column: 1 / -1;
        display: flex;
        align-items: center;
        justify-content: center;
        height: 96px;
        text-align: center;
        color: var(--zui-color-muted-foreground);
    }"
}
