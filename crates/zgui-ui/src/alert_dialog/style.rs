//! The few things an alert dialog does differently from a dialog, in tokens.

use zgui::style;

style! { pub AlertDialogStyle =>
    // Everything else it looks like is a dialog's, and is not restated here. What follows is only
    // what an interruption differs in.

    // A title that is asking a question rather than naming a surface gets an ordinary line box: it
    // is read as a sentence, and often wraps onto a second line, where a line height cut to the
    // glyphs would set the two lines touching.
    ":scope .zui-dialog__title { line-height: var(--zui-type-leading-lg); }"
    ":scope .zui-dialog__header { gap: calc(var(--zui-space-base) * 1.5); }"

    // The small size is for an interruption with nothing to read: an icon, a line, and two
    // answers. It is narrow, everything in it is centred, and its two answers share the width
    // equally rather than huddling at the right — with only two of them and nothing else on the
    // row, a pair of equal halves is the shape that reads as a choice.
    ":scope[data-size=\"sm\"] { max-width: 320px; }"
    ":scope[data-size=\"sm\"] .zui-dialog__header {
        align-items: center;
        text-align: center;
    }"
    ":scope[data-size=\"sm\"] .zui-dialog__footer {
        display: grid;
        grid-template-columns: 1fr 1fr;
    }"
    // Centred with the words it stands above; in the larger sizes the text is left-aligned and the
    // picture stays on the same edge.
    ":scope[data-size=\"sm\"] .zui-alert-dialog__media { align-self: center; }"

    // The picture above the question, when there is one.
    ".zui-alert-dialog__media {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 64px;
        height: 64px;
        margin-bottom: var(--zui-space-sm);
        border-radius: var(--zui-radius-md);
        background-color: var(--zui-color-muted);
    }"
    ".zui-alert-dialog__media .zui-icon { width: 32px; height: 32px; }"
}
