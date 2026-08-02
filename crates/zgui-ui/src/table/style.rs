//! What a table looks like, in tokens.

use zgui::style;

style! { pub TableStyle =>
    // The scroller the table sits in. A table is the one piece of content whose width is decided by
    // what is in it, so the box around it is the one that overflows rather than the page.
    ".zui-table-container {
        position: relative;
        width: 100%;
        overflow-x: auto;
    }"
    // One grid for the whole table rather than a formatting context per section: the column widths
    // are the grid's tracks, so a header cell and the cell three hundred rows below it are the same
    // track and line up without either of them measuring the other.
    ":scope {
        display: grid;
        grid-template-columns: var(--zui-table-columns, 1fr);
        width: 100%;
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        color: var(--zui-color-foreground);
    }"
    // Every row and every section is boxless, which is what makes their cells the grid's own items.
    // A row that generated a box would be one grid item holding all its cells, and every column
    // would then be as wide as the widest cell in whichever row happened to be first.
    ":scope > .zui-table__section { display: contents; }"
    ":scope .zui-table__row { display: contents; }"
    ":scope .zui-table__cell,
      :scope .zui-table__head {
        display: flex;
        align-items: center;
        gap: var(--zui-space-sm);
        border-bottom: 1px solid var(--zui-color-border);
        min-width: 0;
        box-sizing: border-box;
        white-space: nowrap;
        transition: background-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    border-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ":scope .zui-table__cell { padding: var(--zui-space-sm); }"
    // A header cell is a fixed height rather than padding, so a column whose name wraps to nothing
    // still leaves the header the same depth as the one beside it.
    ":scope .zui-table__head {
        height: 40px;
        padding: 0 var(--zui-space-sm);
        font-weight: var(--zui-type-weight-medium);
        color: var(--zui-color-foreground);
        text-align: left;
    }"
    // The header sticks to the cells, not to a row: the row has no box to stick. It needs a paint of
    // its own to stick over, which is the one case where a header cell has a background at all.
    ":scope[data-sticky-header=\"true\"] .zui-table__head {
        position: sticky;
        top: 0;
        z-index: 1;
        background-color: var(--zui-color-background);
    }"
    ":scope .zui-table__cell[data-align=\"end\"],
      :scope .zui-table__head[data-align=\"end\"] {
        justify-content: flex-end;
        text-align: right;
    }"
    ":scope .zui-table__cell[data-align=\"center\"],
      :scope .zui-table__head[data-align=\"center\"] {
        justify-content: center;
        text-align: center;
    }"
    // A boxless row still matches `:hover`, and this is the rule that proves it: the highlight is
    // written against the row and lands on the cells, because the row itself paints nothing.
    ":scope .zui-table__row:hover .zui-table__cell {
        background-color: color-mix(in oklab, var(--zui-color-muted) 50%, transparent);
    }"
    ":scope .zui-table__row[data-selected=\"true\"] .zui-table__cell {
        background-color: var(--zui-color-muted);
    }"
    // The last row of the body has nothing under it to be ruled off from, and a rule there would read
    // as an edge the table does not have.
    ":scope .zui-table__body .zui-table__row:last-child .zui-table__cell {
        border-bottom: none;
    }"
    ":scope .zui-table__footer {
        font-weight: var(--zui-type-weight-medium);
    }"
    ":scope .zui-table__footer .zui-table__cell {
        background-color: color-mix(in oklab, var(--zui-color-muted) 50%, transparent);
        font-weight: var(--zui-type-weight-medium);
    }"
    ":scope .zui-table__footer .zui-table__row:first-child .zui-table__cell {
        border-top: 1px solid var(--zui-color-border);
    }"
    ":scope .zui-table__footer .zui-table__row:last-child .zui-table__cell {
        border-bottom: none;
    }"
    // Under the table wherever it is written, which is what a caption is: the grid orders its items,
    // so a caption declared before the header still lands beneath the last row.
    ":scope .zui-table__caption {
        grid-column: 1 / -1;
        order: 1;
        margin-top: var(--zui-space-lg);
        color: var(--zui-color-muted-foreground);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        text-align: left;
    }"
}
