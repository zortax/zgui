//! What a tab set looks like, in tokens.

use zgui::style;

style! { pub TabsStyle =>
    ":scope { display: flex; flex-direction: column; gap: var(--zui-space-sm); }"
    ":scope[data-orientation=\"vertical\"] { flex-direction: row; }"

    // The strip is exactly as wide as its tabs and no wider: a strip that filled its row would put
    // two tabs at either end of the page with nothing between them.
    ".zui-tabs__list {
        display: inline-flex;
        flex-direction: row;
        width: fit-content;
        height: calc(var(--zui-space-base) * 9);
        align-items: center;
        justify-content: center;
        padding: 3px;
        border-radius: var(--zui-radius-lg);
        color: var(--zui-color-muted-foreground);
    }"
    ".zui-tabs__list[data-variant=\"default\"] { background-color: var(--zui-color-muted); }"
    // The lined strip has no trough at all: what marks the chosen tab is the rule under it, so a
    // filled pill behind it would be a second answer to the same question.
    ".zui-tabs__list[data-variant=\"line\"] {
        gap: var(--zui-space-xs);
        padding: 0;
        border-radius: 0;
        background-color: transparent;
    }"
    ":scope[data-orientation=\"vertical\"] .zui-tabs__list {
        flex-direction: column;
        height: fit-content;
    }"

    ".zui-tabs__trigger {
        position: relative;
        display: inline-flex;
        flex: 1 1 auto;
        align-items: center;
        justify-content: center;
        gap: calc(var(--zui-space-base) * 1.5);
        height: calc(100% - 1px);
        padding: var(--zui-space-xs) var(--zui-space-sm);
        border: 1px solid transparent;
        border-radius: var(--zui-radius-md);
        background-color: transparent;
        color: var(--zui-color-control-tab-ink);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-medium);
        line-height: var(--zui-type-leading-sm);
        white-space: nowrap;
        transition: color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    background-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    box-shadow var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ":scope[data-orientation=\"vertical\"] .zui-tabs__trigger {
        width: 100%;
        justify-content: flex-start;
    }"
    ".zui-tabs__trigger:hover { color: var(--zui-color-foreground); }"
    ".zui-tabs__trigger[data-state=\"active\"] {
        border-color: var(--zui-color-control-tab-border);
        background-color: var(--zui-color-control-tab-fill);
        color: var(--zui-color-foreground);
    }"
    ".zui-tabs__list[data-variant=\"default\"] .zui-tabs__trigger[data-state=\"active\"] {
        box-shadow: var(--zui-shadow-xs);
    }"
    ".zui-tabs__list[data-variant=\"line\"] .zui-tabs__trigger,
     .zui-tabs__list[data-variant=\"line\"] .zui-tabs__trigger[data-state=\"active\"] {
        background-color: transparent;
        border-color: transparent;
        box-shadow: none;
    }"
    ".zui-tabs__trigger:focus-visible {
        outline: 1px solid var(--zui-color-ring);
        border-color: var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
    ".zui-tabs__trigger:disabled { opacity: 0.5; pointer-events: none; }"

    // The lined strip's mark: a rule against the strip's far edge, drawn on every tab and made
    // visible on the chosen one. On every tab rather than moved between them, because it is an
    // opacity that fades — a bar that slid would have to know where the other tabs are.
    ".zui-tabs__trigger::after {
        content: \"\";
        position: absolute;
        background-color: var(--zui-color-foreground);
        opacity: 0;
        transition: opacity var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ":scope[data-orientation=\"horizontal\"] .zui-tabs__trigger::after {
        left: 0;
        right: 0;
        bottom: -5px;
        height: 2px;
    }"
    ":scope[data-orientation=\"vertical\"] .zui-tabs__trigger::after {
        top: 0;
        bottom: 0;
        right: -4px;
        width: 2px;
    }"
    ".zui-tabs__list[data-variant=\"line\"] .zui-tabs__trigger[data-state=\"active\"]::after {
        opacity: 1;
    }"

    // On a dark surface an unchosen tab recedes by being the muted colour outright rather than by
    // being a fraction of the foreground: the foreground there is near white, and six tenths of it
    // is a grey too bright to read as inactive.
        ".zui-tabs__content { display: flex; flex: 1 1 auto; flex-direction: column; }"
    ".zui-tabs__content[data-state=\"inactive\"] { display: none; }"
    ".zui-tabs__content:focus-visible { outline: none; }"
}
