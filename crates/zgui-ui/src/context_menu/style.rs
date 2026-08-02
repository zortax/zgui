//! What the area a context menu opens over looks like, in tokens.

use zgui::style;

style! { pub ContextMenuStyle =>
    // The area is positioned so the anchor inside it can be placed at the pointer. It has no
    // appearance of its own beyond that.
    ":scope { position: relative; display: flex; flex-direction: column; }"
    // A point rather than a box: what the menu is anchored to is where the pointer was, and a
    // surface placed against something with size would be offset by that size.
    ".zui-context-menu__anchor { position: absolute; width: 0px; height: 0px; }"
}
