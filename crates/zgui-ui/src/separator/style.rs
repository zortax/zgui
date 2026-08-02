//! What a separator looks like, in tokens.

use zgui::style;

style! { pub SeparatorStyle =>
    ":scope { background-color: var(--zui-color-border); flex: none; }"
    ":scope[data-orientation=\"horizontal\"] { height: 1px; width: 100%; }"
    ":scope[data-orientation=\"vertical\"] { width: 1px; align-self: stretch; }"
}
