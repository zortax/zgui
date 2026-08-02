//! What an avatar looks like, in tokens.

use zgui::style;

style! { pub AvatarStyle =>
    ":scope {
        position: relative;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        overflow: hidden;
        flex: none;
        width: 32px;
        height: 32px;
        border-radius: var(--zui-radius-full);
        background-color: var(--zui-color-muted);
        color: var(--zui-color-muted-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-medium);
        line-height: var(--zui-type-leading-sm);
        user-select: none;
    }"
    ":scope[data-size=\"sm\"] { width: 24px; height: 24px; font-size: var(--zui-type-size-xs); }"
    ":scope[data-size=\"lg\"] { width: 40px; height: 40px; }"

    ":scope > .zui-avatar__image {
        position: absolute;
        inset: 0;
        width: 100%;
        height: 100%;
        aspect-ratio: 1;
        object-fit: cover;
    }"
    // A picture that failed to load is taken out of the flow rather than left as a broken box, and
    // `:broken` is the engine's own answer to that — not a load handler and a signal beside it.
    ":scope > .zui-avatar__image:broken { display: none; }"

    ":scope > .zui-avatar__fallback {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 100%;
        height: 100%;
        border-radius: var(--zui-radius-full);
        background-color: var(--zui-color-muted);
        color: var(--zui-color-muted-foreground);
    }"

    // The badge is pinned to the bottom corner and lifted above the picture, and the ring it wears
    // is the page's own colour: an unringed dot on a dark photograph disappears into it.
    ":scope > .zui-avatar__badge {
        position: absolute;
        right: 0;
        bottom: 0;
        z-index: 10;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 10px;
        height: 10px;
        border-radius: var(--zui-radius-full);
        background-color: var(--zui-color-primary);
        color: var(--zui-color-primary-foreground);
        box-shadow: 0 0 0 2px var(--zui-color-background);
        user-select: none;
    }"
    ":scope > .zui-avatar__badge .zui-icon { --zui-icon-md: 8px; }"
    ":scope[data-size=\"sm\"] > .zui-avatar__badge { width: 8px; height: 8px; }"
    // At eight pixels across there is no room for a drawing, only for the dot itself.
    ":scope[data-size=\"sm\"] > .zui-avatar__badge .zui-icon { display: none; }"
    ":scope[data-size=\"lg\"] > .zui-avatar__badge { width: 12px; height: 12px; }"

    // A stack: each face laps the one before it, and the ring in the page colour is what keeps the
    // overlap legible rather than turning the row into one smear.
    ".zui-avatar-group { display: flex; flex-direction: row; align-items: center; }"
    ".zui-avatar-group > * { box-shadow: 0 0 0 2px var(--zui-color-background); }"
    ".zui-avatar-group > *:not(:first-child) { margin-left: -8px; }"

    ".zui-avatar-group__count {
        position: relative;
        display: flex;
        flex: none;
        align-items: center;
        justify-content: center;
        width: 32px;
        height: 32px;
        border-radius: var(--zui-radius-full);
        background-color: var(--zui-color-muted);
        color: var(--zui-color-muted-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
    }"
    // The count takes the size of the faces beside it rather than one of its own, so a group never
    // ends in a disc that is the wrong size for the row it closes.
    ".zui-avatar-group[data-size=\"sm\"] > .zui-avatar-group__count {
        width: 24px;
        height: 24px;
        font-size: var(--zui-type-size-xs);
        --zui-icon-md: 12px;
    }"
    ".zui-avatar-group[data-size=\"lg\"] > .zui-avatar-group__count {
        width: 40px;
        height: 40px;
        --zui-icon-md: 20px;
    }"
}
