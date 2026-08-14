//! What a page of settings looks like, in tokens.

use zgui::prelude::install_stylesheet;
use zgui::style;

/// What the settings' rules are installed under.
const SHEET: &str = "zui-settings";

/// Puts the sheet in place.
///
/// Any part will do it: a pane written on its own still wants the rules for the groups inside it,
/// and installing a sheet twice is free.
pub(crate) fn install() {
    install_stylesheet(SHEET, SettingsStyle::CSS);
}

style! { pub SettingsStyle =>
    // Two columns that scroll separately, so a long pane never carries the page list off the top
    // of the window with it. `min-height: 0` on both is what lets either one clip rather than
    // growing the row it is in.
    ":scope {
        display: flex;
        flex-direction: row;
        align-items: stretch;
        min-height: 0;
        width: 100%;
        color: var(--zui-color-foreground);
    }"

    ".zui-settings__pages {
        display: flex;
        flex: 0 0 auto;
        flex-direction: column;
        gap: var(--zui-space-xs);
        min-height: 0;
        width: 200px;
        padding: var(--zui-space-md);
        overflow-y: auto;
        border-right: 1px solid var(--zui-color-border);
        background-color: var(--zui-color-sidebar);
    }"

    ".zui-settings__page {
        display: flex;
        flex: 0 0 auto;
        flex-direction: row;
        align-items: center;
        gap: var(--zui-space-sm);
        padding: var(--zui-space-sm) var(--zui-space-md);
        border-radius: var(--zui-radius-md);
        background-color: transparent;
        color: var(--zui-color-sidebar-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-medium);
        line-height: var(--zui-type-leading-sm);
        text-align: start;
        transition: background-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ".zui-settings__page:hover { background-color: var(--zui-color-sidebar-accent); }"
    ".zui-settings__page[data-state=\"active\"] {
        background-color: var(--zui-color-sidebar-accent);
        color: var(--zui-color-sidebar-accent-foreground);
    }"
    ".zui-settings__page:focus-visible {
        outline: 1px solid var(--zui-color-sidebar-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-sidebar-ring) 50%, transparent);
    }"
    ".zui-settings__page[data-disabled=\"true\"] { opacity: 0.5; pointer-events: none; }"

    ".zui-settings__pane {
        display: flex;
        flex: 1 1 auto;
        flex-direction: column;
        gap: var(--zui-space-3xl);
        min-width: 0;
        min-height: 0;
        padding: var(--zui-space-xl);
        overflow-y: auto;
    }"
    ".zui-settings__pane[data-state=\"inactive\"] { display: none; }"
    ".zui-settings__pane:focus-visible { outline: none; }"

    ".zui-settings__group {
        display: flex;
        flex-direction: column;
        gap: var(--zui-space-sm);
    }"
    ".zui-settings__group-label {
        display: block;
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-md);
        font-weight: var(--zui-type-weight-semibold);
        line-height: var(--zui-type-leading-md);
    }"
    ".zui-settings__group-description {
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        color: var(--zui-color-muted-foreground);
    }"

    // A hairline between one row and the next, and none above the first: the rule belongs to the
    // pair rather than to either row, so a group of one item is a row with nothing drawn round it.
    ".zui-settings__item { padding-block: var(--zui-space-md); }"
    ".zui-settings__item + .zui-settings__item { border-top: 1px solid var(--zui-color-border); }"
    // The words take the room that is going and the control takes what it needs, which is what
    // lines every control in a pane up against the same trailing edge.
    ".zui-settings__item-control {
        display: flex;
        flex: 0 0 auto;
        align-items: center;
        justify-content: flex-end;
        gap: var(--zui-space-sm);
    }"
    ".zui-settings__item-description {
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-normal);
        line-height: 1.5;
        color: var(--zui-color-muted-foreground);
    }"
}
