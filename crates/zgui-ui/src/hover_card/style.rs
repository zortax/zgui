//! What a hover card looks like, in tokens.

use zgui::style;

style! { pub HoverCardStyle =>
    // A popover's surface at a preview's width: narrow enough that the eye takes it in without
    // reading, because nobody asked for it.
    ":scope {
        width: 256px;
        padding: var(--zui-space-lg);
        outline: none;
    }"
    ".zui-hover-card__trigger { display: inline-flex; align-items: center; }"
}
