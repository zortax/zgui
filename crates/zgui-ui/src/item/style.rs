//! What a row of content looks like, in tokens.

use zgui::style;

style! { pub ItemStyle =>
    ":scope {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--zui-space-lg);
        padding: var(--zui-space-lg);
        border: 1px solid transparent;
        border-radius: var(--zui-radius-md);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        background-color: transparent;
        transition-property: background-color, border-color, color;
        transition-duration: var(--zui-motion-duration-fast);
        transition-timing-function: var(--zui-motion-ease-standard);
    }"
    ":scope[data-variant=\"outline\"] { border-color: var(--zui-color-border); }"
    // Half strength: a muted row is a shade off the page rather than a panel on it, and the muted
    // colour at full strength is heavy enough to read as a card.
    ":scope[data-variant=\"muted\"] {
        background-color: color-mix(in oklab, var(--zui-color-muted) 50%, transparent);
    }"
    ":scope[data-size=\"sm\"] {
        gap: calc(var(--zui-space-base) * 2.5);
        padding: var(--zui-space-md) var(--zui-space-lg);
    }"
    ":scope:focus-visible {
        border-color: var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
}

style! { pub ItemGroupStyle =>
    ":scope { display: flex; flex-direction: column; }"
    ".zui-item__separator {
        flex-shrink: 0;
        height: 1px;
        margin: 0;
        background-color: var(--zui-color-border);
    }"
}

style! { pub ItemPartStyle =>
    ".zui-item__media {
        display: flex;
        flex-shrink: 0;
        align-items: center;
        justify-content: center;
        gap: var(--zui-space-sm);
        background-color: transparent;
    }"
    ".zui-item__media .zui-icon { pointer-events: none; }"
    // A row with a description is two lines tall, and a mark centred against two lines sits below
    // the title it belongs to. Against a title alone it is centred, which is what the default is.
    ".zui-item--described .zui-item__media {
        align-self: flex-start;
        transform: translateY(2px);
    }"
    ".zui-item__media[data-variant=\"icon\"] {
        width: 32px;
        height: 32px;
        border: 1px solid var(--zui-color-border);
        border-radius: var(--zui-radius-sm);
        background-color: var(--zui-color-muted);
    }"
    ".zui-item__media[data-variant=\"image\"] {
        width: 40px;
        height: 40px;
        border-radius: var(--zui-radius-sm);
        overflow: hidden;
    }"
    ".zui-item__media[data-variant=\"image\"] > * {
        width: 100%;
        height: 100%;
        object-fit: cover;
    }"
    ".zui-item__content {
        display: flex;
        flex: 1;
        flex-direction: column;
        gap: var(--zui-space-xs);
    }"
    // A second block of words beside the first is a sidenote, not a second column that shares the
    // room: it takes what it needs and leaves the rest to the block that came before it.
    ".zui-item__content + .zui-item__content { flex: none; }"
    ".zui-item__title {
        display: flex;
        width: fit-content;
        align-items: center;
        gap: var(--zui-space-sm);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-medium);
        line-height: 1.375;
    }"
    ".zui-item__description {
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-normal);
        line-height: 1.5;
        color: var(--zui-color-muted-foreground);
    }"
    ".zui-item__actions { display: flex; align-items: center; gap: var(--zui-space-sm); }"
    ".zui-item__header, .zui-item__footer {
        display: flex;
        flex-basis: 100%;
        align-items: center;
        justify-content: space-between;
        gap: var(--zui-space-sm);
    }"
}
