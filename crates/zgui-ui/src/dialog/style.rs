//! What a dialog looks like, in tokens.

use zgui::style;

style! { pub DialogStyle =>
    // Centred on the window rather than on anything in the tree, because what a dialog is centred
    // in is the window: it has been portalled out of whatever laid its trigger out.
    //
    // The width is stated twice, and both are needed. `100%` is what it takes when the window is
    // narrow; the maximum is what stops it stretching across a wide one. The height is capped a
    // gutter short of the window so a long dialog never runs off the top and bottom of it.
    ":scope {
        position: fixed;
        left: 50%;
        top: 50%;
        /* Where it sits, kept apart from how it enters, so the shared surface motion
           composes with it instead of replacing it. */
        --zui-surface-place: translate(-50%, -50%);
        width: 100%;
        max-width: 512px;
        max-height: calc(100% - 32px);
        gap: var(--zui-space-lg);
        padding: var(--zui-space-xl);
        outline: none;
        /* Rounder, lighter and lifted further off the page than the popover the shared
           surface paints by default. */
        --zui-surface-radius: var(--zui-radius-lg);
        --zui-surface-fill: var(--zui-color-background);
        --zui-surface-ink: var(--zui-color-foreground);
        --zui-surface-shadow: var(--zui-shadow-lg);
    }"
    // A dialog fades and zooms in place, and takes half again as long over it as a menu does: it is
    // the whole window changing rather than a panel appearing beside a button.
    ":scope {
        --zui-surface-enter-duration: var(--zui-motion-duration-slow);
        --zui-surface-exit-duration: var(--zui-motion-duration-slow);
    }"

    ".zui-dialog__header {
        display: flex;
        flex-direction: column;
        gap: var(--zui-space-sm);
        text-align: left;
    }"
    // The title's line box is the height of its glyphs and no more. A taller one would push the
    // description down by an amount that has nothing to do with the gap the header asked for, and
    // a title is one line by construction.
    ".zui-dialog__title {
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-lg);
        font-weight: var(--zui-type-weight-semibold);
        line-height: 1;
    }"
    ".zui-dialog__description {
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        color: var(--zui-color-muted-foreground);
    }"
    ".zui-dialog__footer {
        display: flex;
        flex-direction: row;
        justify-content: flex-end;
        align-items: center;
        gap: var(--zui-space-sm);
    }"

    // The dismiss control sits in the surface's own corner rather than in the header, so a dialog
    // with no header still has one. It is drawn at seven tenths and comes up to full under the
    // pointer, which is the whole of its hover state: a cross that also grew a background would be
    // the loudest thing on a surface whose point is what is written on it.
    ".zui-dialog__dismiss {
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
    ".zui-dialog__dismiss:hover { opacity: 1; }"
    ".zui-dialog__dismiss:focus-visible {
        outline: 2px solid var(--zui-color-ring);
        outline-offset: 2px;
    }"
    ".zui-dialog__dismiss:disabled { pointer-events: none; }"
}
