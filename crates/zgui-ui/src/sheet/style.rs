//! What a sheet and a drawer look like, in tokens.

use zgui::style;

style! { pub SheetStyle =>
    // Pinned to one edge of the window and stretched along it. Which edge is a `data-` attribute
    // rather than four components, so a sheet that changes side changes an attribute.
    //
    // The panel carries no padding of its own. Everything in a sheet is a full-width band — a
    // header, a scrolling body, a footer at the bottom — and a panel that inset its contents could
    // not draw one that reaches the edges. The bands pad themselves instead.
    ":scope {
        position: fixed;
        display: flex;
        flex-direction: column;
        gap: var(--zui-space-lg);
        padding: 0;
        /* Square, page-coloured and lifted, with the one border it keeps drawn per side below:
           a panel pinned to an edge has three edges that are not edges of anything. */
        --zui-surface-border: none;
        --zui-surface-radius: 0px;
        --zui-surface-fill: var(--zui-color-background);
        --zui-surface-ink: var(--zui-color-foreground);
        --zui-surface-shadow: var(--zui-shadow-lg);
    }"

    // A sheet does not fade and does not zoom: it slides its whole width in from the edge it is
    // pinned to, and back out the same way. It comes in slowly and leaves faster, which is the
    // shape of every panel that is *arriving* somewhere rather than appearing beside something —
    // the entrance is the part worth watching, the exit is a decision already taken.
    ":scope {
        --zui-surface-enter-opacity: 1;
        --zui-surface-enter-scale: 1;
        --zui-surface-exit-opacity: 1;
        --zui-surface-exit-scale: 1;
        --zui-surface-enter-duration: var(--zui-motion-duration-slowest);
        --zui-surface-exit-duration: var(--zui-motion-duration-slower);
        --zui-surface-enter-ease: var(--zui-motion-ease-standard);
        --zui-surface-exit-ease: var(--zui-motion-ease-standard);
    }"

    ":scope[data-side=\"right\"] {
        right: 0; top: 0; bottom: 0;
        width: 75%;
        max-width: 384px;
        border-left: 1px solid var(--zui-color-border);
        --zui-surface-enter-x: 100%;
        --zui-surface-exit-x: 100%;
    }"
    ":scope[data-side=\"left\"] {
        left: 0; top: 0; bottom: 0;
        width: 75%;
        max-width: 384px;
        border-right: 1px solid var(--zui-color-border);
        --zui-surface-enter-x: -100%;
        --zui-surface-exit-x: -100%;
    }"
    ":scope[data-side=\"top\"] {
        left: 0; right: 0; top: 0;
        height: auto;
        border-bottom: 1px solid var(--zui-color-border);
        --zui-surface-enter-y: -100%;
        --zui-surface-exit-y: -100%;
    }"
    ":scope[data-side=\"bottom\"] {
        left: 0; right: 0; bottom: 0;
        height: auto;
        border-top: 1px solid var(--zui-color-border);
        --zui-surface-enter-y: 100%;
        --zui-surface-exit-y: 100%;
    }"

    // The bands. A header pads itself and sits at the top; a footer pads itself and is pushed to
    // the bottom by whatever is between them, so a sheet holding one short paragraph still has its
    // answers where the eye expects them rather than halfway up the panel.
    ".zui-sheet__header {
        display: flex;
        flex-direction: column;
        gap: calc(var(--zui-space-base) * 1.5);
        padding: var(--zui-space-lg);
    }"
    ".zui-sheet__footer {
        display: flex;
        flex-direction: column;
        gap: var(--zui-space-sm);
        padding: var(--zui-space-lg);
        margin-top: auto;
    }"
    ".zui-sheet__title {
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-md);
        font-weight: var(--zui-type-weight-semibold);
        line-height: var(--zui-type-leading-md);
        color: var(--zui-color-foreground);
    }"
    ".zui-sheet__description {
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        color: var(--zui-color-muted-foreground);
    }"

    // The cross in a sheet's own corner: the same control as a dialog's, down to the seven tenths
    // it rests at, and in the same place whether or not the sheet has a header.
    ".zui-sheet__dismiss {
        position: absolute;
        right: var(--zui-space-lg);
        top: var(--zui-space-lg);
        display: inline-flex;
        align-items: center;
        justify-content: center;
        border: none;
        background-color: transparent;
        border-radius: 2px;
        color: inherit;
        opacity: 0.7;
        transition: opacity var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ".zui-sheet__dismiss:hover { opacity: 1; }"
    ".zui-sheet__dismiss:focus-visible {
        outline: 2px solid var(--zui-color-ring);
        outline-offset: 2px;
    }"
    ".zui-sheet__dismiss:disabled { pointer-events: none; }"

    // A drawer is a sheet that keeps the two corners facing into the window, so it reads as
    // something pulled out from the edge rather than a wall that appeared there. It also stops
    // short of the far edge, which is what leaves enough of the page behind it to still be a page.
    ".zui-drawer[data-side=\"bottom\"] {
        margin-top: var(--zui-space-3xl);
        max-height: 80vh;
        border-top-left-radius: var(--zui-radius-lg);
        border-top-right-radius: var(--zui-radius-lg);
    }"
    ".zui-drawer[data-side=\"top\"] {
        margin-bottom: var(--zui-space-3xl);
        max-height: 80vh;
        border-bottom-left-radius: var(--zui-radius-lg);
        border-bottom-right-radius: var(--zui-radius-lg);
    }"
    // A drawer along an edge that runs the width of the window is read from the middle, so what is
    // in its header is centred. One down the side is read as a column and is not.
    ".zui-drawer[data-side=\"bottom\"] .zui-sheet__header,
     .zui-drawer[data-side=\"top\"] .zui-sheet__header {
        align-items: center;
        text-align: center;
    }"
    // The bar that says *this can be pulled*. It says it only where it is true: a drawer along the
    // bottom is dragged down, and one down the side is not dragged at all.
    ".zui-drawer__handle {
        display: none;
        width: 100px;
        height: 8px;
        flex-shrink: 0;
        border-radius: 9999px;
        background-color: var(--zui-color-muted);
        margin-left: auto;
        margin-right: auto;
        margin-top: var(--zui-space-lg);
    }"
    ".zui-drawer[data-side=\"bottom\"] .zui-drawer__handle { display: block; }"
}
