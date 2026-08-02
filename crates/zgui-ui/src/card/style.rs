//! What a card looks like, in tokens.

use zgui::style;

style! { pub CardStyle =>
    // The card itself has no side padding: each piece pads its own sides, so a piece meant to run
    // edge to edge — a picture, a table, a separator — is written without having to undo anything.
    // The vertical padding and the gap between the pieces are the card's.
    ":scope {
        display: flex;
        flex-direction: column;
        gap: var(--zui-space-xl);
        padding-block: var(--zui-space-xl);
        border: 1px solid var(--zui-color-border);
        border-radius: var(--zui-radius-xl);
        background-color: var(--zui-color-card);
        color: var(--zui-color-card-foreground);
        box-shadow: var(--zui-shadow-sm);
    }"

    // A grid rather than a column, because the action sits beside the title *and* the description
    // rather than after either of them: one cell spanning both rows, which no stacking of rows and
    // columns expresses. With no action there is one column and the grid reads as a column.
    ".zui-card__header {
        display: grid;
        grid-auto-rows: min-content;
        grid-template-rows: auto auto;
        align-items: start;
        gap: var(--zui-space-sm);
        padding-inline: var(--zui-space-xl);
    }"
    ".zui-card__header--action { grid-template-columns: 1fr auto; }"
    ".zui-card__title {
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-md);
        font-weight: var(--zui-type-weight-semibold);
        line-height: 1;
    }"
    ".zui-card__description {
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        color: var(--zui-color-muted-foreground);
    }"
    ".zui-card__action {
        grid-column-start: 2;
        grid-row: 1 / span 2;
        align-self: start;
        justify-self: end;
    }"
    ".zui-card__content { padding-inline: var(--zui-space-xl); }"
    ".zui-card__footer {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: var(--zui-space-sm);
        padding-inline: var(--zui-space-xl);
    }"
}
