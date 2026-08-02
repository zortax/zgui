//! What an icon looks like, in tokens.

use zgui::style;

style! { pub IconStyle =>
    // An icon is a box in a line of text: it does not stretch when its row does, and it does not
    // shrink when the label beside it is long. Both are `flex: none`, and leaving it out is how an
    // icon beside a growing label ends up an ellipse.
    ":scope {
        display: inline-block;
        flex: none;
        width: var(--zui-icon-size);
        height: var(--zui-icon-size);
        --zui-icon-size: var(--zui-icon-md, 16px);
    }"
    // The fill is deliberately not declared. A drawing with no fill of its own takes the element's
    // computed `color`, which is what makes an icon follow the text it sits in — and what makes
    // `.button:hover .zui-icon` a colour change rather than a mechanism.
    ":scope[data-size=\"sm\"] { --zui-icon-size: var(--zui-icon-sm, 12px); }"
    ":scope[data-size=\"lg\"] { --zui-icon-size: var(--zui-icon-lg, 20px); }"
    ":scope[data-size=\"xl\"] { --zui-icon-size: var(--zui-icon-xl, 24px); }"
}
