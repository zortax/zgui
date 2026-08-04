//! What the inspector looks like.
//!
//! Its own sheet rather than the component library's, for one reason: the inspector is shown beside
//! an application whose sheet it knows nothing about, and a panel that inherited the page's own
//! rules would change appearance from one application to the next — which is the one thing a
//! diagnostic surface must not do. Every rule here is scoped under `.zgui-devtools`, every colour is
//! literal, and nothing is a custom property somebody else's sheet could redefine.
//!
//! **The dock is a flex row, and every rule that makes it one is behind a class the open panel
//! adds.** A closed inspector must leave the document it is linked into exactly as it found it, so
//! the host is an ordinary wrapper until the moment the panel appears; only then does it take the
//! window's height, turn into a row, and give the application its own scroll. The panel's own
//! height is then nobody's declaration — it is the cross size of the line it sits on, which is the
//! viewport, which is the whole reason the arrangement is worth having.

/// What the inspector's sheet is installed under.
///
/// Prefixed like every class in it, because sheet names are shared across a whole document and the
/// application the panel is docked into knows nothing about this one.
pub(crate) const SHEET_NAME: &str = "zgui-devtools";

/// The inspector's style sheet.
///
/// The panel installs this itself, from its own body, under the name `zgui-devtools` — so an
/// application wires the inspector in without knowing the sheet exists. Installing by name is
/// idempotent, which is what makes that safe to do unconditionally from a view that may mount more
/// than once.
pub(crate) const SHEET: &str = zgui::css!(
    // The root is a block of the window's size, and a block's child is as tall as its own content.
    // So the wrapper the inspector puts around an application has to say it fills the window,
    // whether the panel is showing or not: without it, a view that filled the window on its own
    // gains a strip of empty space under it the moment the inspector is linked in — and only while
    // the panel is *shut*, which is the state an application spends most of its life in.
    ".zgui-devtools-host {
        flex-direction: column;
        align-items: stretch;
        width: 100%;
        height: 100%;
     }
     .zgui-devtools-host-docked { flex-direction: row }
     .zgui-devtools-app {
        flex-direction: column;
        flex-grow: 1;
        flex-shrink: 1;
        min-width: 0;
        min-height: 0;
     }
     .zgui-devtools-app-docked { overflow: auto }
     .zgui-devtools__divider {
        display: flex;
        width: 7px;
        flex-grow: 0;
        flex-shrink: 0;
        align-self: stretch;
        align-items: center;
        background-color: transparent;
     }
     .zgui-devtools__divider-line {
        width: 1px;
        align-self: stretch;
        background-color: transparent;
     }
     .zgui-devtools__divider:hover .zgui-devtools__divider-line { background-color: #2f6bff }
     .zgui-devtools {
        width: 420px;
        flex-grow: 0;
        flex-shrink: 0;
        align-self: stretch;
        flex-direction: column;
        background-color: #0f1116;
        border-left: 1px solid #2a3040;
        color: #d8dee9;
        font-family: monospace;
        font-size: 12px;
     }
     .zgui-devtools__bar {
        flex-direction: row;
        align-items: center;
        flex-wrap: wrap;
        gap: 2px;
        padding: 6px;
        border-bottom: 1px solid #2a3040;
        background-color: #151924;
     }
     /* `control` is a block in the user-agent sheet, so a flex direction on it means nothing until
        it is made a flex container — without `display: flex` the icon stacks *above* the label
        rather than sitting in front of it. */
     .zgui-devtools__tab {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: 5px;
        padding: 4px 9px;
        border-radius: 5px;
        color: #97a3b6;
        background-color: transparent;
        flex-grow: 0;
        flex-shrink: 0;
     }
     .zgui-devtools__tab-icon {
        /* Block, because an atomic inline does not take an explicit width in this build. */
        display: block;
        width: 13px;
        height: 13px;
        flex-grow: 0;
        flex-shrink: 0;
        --zgui-fill: transparent;
        --zgui-stroke: #97a3b6;
        --zgui-stroke-width: 1.6px;
     }
     .zgui-devtools__tab-label { flex-grow: 0; flex-shrink: 0 }
     .zgui-devtools__tab:hover .zgui-devtools__tab-icon { --zgui-stroke: #d8dee9 }
     .zgui-devtools__tab-on .zgui-devtools__tab-icon { --zgui-stroke: #ffffff }
     .zgui-devtools__tab:hover { background-color: #1d2432 }
     .zgui-devtools__tab-on { background-color: #2f6bff; color: #ffffff }
     .zgui-devtools__spacer { flex-grow: 1 }
     .zgui-devtools__toggle {
        display: flex;
        flex-direction: row;
        padding: 4px 6px;
        border-radius: 5px;
        border: 1px solid #2a3040;
        color: #97a3b6;
        align-items: center;
        flex-grow: 0;
        flex-shrink: 0;
     }
     .zgui-devtools__toggle-on { background-color: #b45309; color: #ffffff }
     .zgui-devtools__icon {
        /* `block`, not the `inline-block` a drawing is by default: this engine does not size an
           atomic inline from an explicit width, so an icon left inline lays out at nothing by
           nothing and the button draws blank. */
        display: block;
        width: 16px;
        height: 16px;
        flex-grow: 0;
        flex-shrink: 0;
        --zgui-fill: transparent;
        --zgui-stroke: #97a3b6;
        --zgui-stroke-width: 1.4px;
     }
     .zgui-devtools__toggle:hover .zgui-devtools__icon { --zgui-stroke: #d8dee9 }
     .zgui-devtools__toggle-on .zgui-devtools__icon { --zgui-stroke: #ffffff }
     /* The strip below the tab bar. It holds whichever tab is showing and scrolls *nothing*: each
        tab brings its own scrolling body, and a scroller wrapping a scroller is two gutters down
        the right-hand edge and a page that keeps moving after the inner one has hit its end. */
     .zgui-devtools__tabs {
        flex-direction: column;
        flex-grow: 1;
        flex-shrink: 1;
        min-height: 0;
        overflow: hidden;
     }
     .zgui-devtools__body {
        flex-direction: column;
        gap: 10px;
        padding: 10px;
        overflow-y: auto;
        overflow-x: hidden;
        flex-grow: 1;
        flex-shrink: 1;
        min-height: 0;
        min-width: 0;
     }
     /* A scrolling column is still a flex column, and a flex item's default is to *shrink* rather
        than overflow — so a body with more in it than fits squashed its own rows until the text was
        unreadable, instead of scrolling past them. Everything that is a line of the panel keeps its
        height and lets the body scroll. */
     .zgui-devtools__body > * { flex-shrink: 0 }
     .zgui-devtools__head { color: #7ee3ff; padding-bottom: 2px }
     .zgui-devtools__row {
        flex-direction: row;
        gap: 8px;
        align-items: baseline;
        flex-grow: 0;
        flex-shrink: 0;
     }
     .zgui-devtools__key { width: 168px; color: #8b97ab }
     .zgui-devtools__swatch {
        width: 10px;
        height: 10px;
        flex-grow: 0;
        flex-shrink: 0;
        border-radius: 2px;
        border: 1px solid #2a3040;
     }
     .zgui-devtools__value { flex-grow: 1; color: #e8edf6 }
     .zgui-devtools__value-quiet { flex-grow: 1; color: #71809a }
     .zgui-devtools__note { color: #71809a }
     /* One stage per line: the words take the room they need, the mark's own name sits behind them
        and gives it up first, and the cost is pinned to the right where the eye scans for it. */
     .zgui-devtools__stage { color: #e8edf6; flex-grow: 0; flex-shrink: 0 }
     .zgui-devtools__stage-mark {
        color: #5c6a80;
        flex-grow: 1;
        flex-shrink: 1;
        min-width: 0;
     }
     .zgui-devtools__cost { color: #e8edf6; flex-grow: 0; flex-shrink: 0 }
     .zgui-devtools__box {
        flex-direction: column;
        gap: 2px;
        padding: 8px;
        border: 1px dashed #8b6f2f;
        background-color: #191712;
     }
     .zgui-devtools__box-border { border-color: #e0c25a; background-color: #201b10 }
     .zgui-devtools__box-padding { border-color: #3aa76d; background-color: #0f1a14 }
     .zgui-devtools__box-content { border-color: #5c9eff; background-color: #101526 }
     .zgui-devtools__note-border { color: #e0c25a }
     .zgui-devtools__note-padding { color: #3aa76d }
     .zgui-devtools__note-content { color: #5c9eff }
     /* The frame-time graph. A drawing rather than a row of boxes: it is one line over a thousand
        samples, and a thousand boxes is a thousand elements in the document the panel is measuring.
        It fits its own box, so it cannot push the tab sideways however many samples it holds. */
     .zgui-devtools__plot {
        flex-direction: row;
        align-items: stretch;
        gap: 6px;
        height: 96px;
        flex-grow: 0;
        flex-shrink: 0;
     }
     /* The scale beside the plot. `space-between` puts the ceiling at the top, the budget across
        the middle where the rule is drawn, and zero on the floor. */
     .zgui-devtools__axis {
        flex-direction: column;
        justify-content: space-between;
        align-items: flex-end;
        flex-grow: 0;
        flex-shrink: 0;
        width: 52px;
        color: #5c6a80;
     }
     .zgui-devtools__axis-budget { color: #8b97ab }
     .zgui-devtools__graph {
        display: block;
        flex-grow: 1;
        flex-shrink: 1;
        min-width: 0;
        height: 96px;
        background-color: #0b0d12;
        border-radius: 3px;
        --zgui-fill: transparent;
        --zgui-stroke: #3aa76d;
        --zgui-stroke-width: 1px;
     }
     .zgui-devtools__strip { flex-direction: row; height: 16px; gap: 1px }
     .zgui-devtools__slice { background-color: #5c6a80; height: 16px }
     .zgui-devtools__slice-events { background-color: #7c6ce0 }
     .zgui-devtools__slice-style { background-color: #e05c8a }
     .zgui-devtools__slice-layout { background-color: #3aa76d }
     .zgui-devtools__slice-paint { background-color: #e0a83c }
     .zgui-devtools__slice-render { background-color: #3f7bdb }
     .zgui-devtools__slice-gpu { background-color: #46c8d8 }
     .zgui-devtools__slice-other { background-color: #5c6a80 }
     .zgui-devtools__slice-slow { border-bottom: 3px solid #ff4d61 }
     .zgui-devtools__value-slow { color: #ff8091 }
     .zgui-devtools__legend { flex-direction: row; flex-wrap: wrap; gap: 4px }
     .zgui-devtools__dot {
        width: 8px;
        height: 8px;
        border-radius: 2px;
        flex-grow: 0;
        flex-shrink: 0;
        background-color: #5c6a80;
     }
     .zgui-devtools__dot-events { background-color: #7c6ce0 }
     .zgui-devtools__dot-style { background-color: #e05c8a }
     .zgui-devtools__dot-layout { background-color: #3aa76d }
     .zgui-devtools__dot-paint { background-color: #e0a83c }
     .zgui-devtools__dot-render { background-color: #3f7bdb }
     .zgui-devtools__dot-gpu { background-color: #46c8d8 }
     .zgui-devtools__dot-other { background-color: #5c6a80 }
     .zgui-devtools__meter { flex-direction: row; height: 14px; gap: 1px; border-radius: 3px }
     .zgui-devtools__seg { height: 14px; background-color: #5c6a80 }
     .zgui-devtools__seg-fixed { background-color: #5c6a80 }
     .zgui-devtools__seg-targets { background-color: #3f7bdb }
     .zgui-devtools__seg-scratch { background-color: #7c6ce0 }
     .zgui-devtools__seg-atlases { background-color: #e0a83c }
     .zgui-devtools__seg-buffers { background-color: #46c8d8 }
     .zgui-devtools__track {
        flex-direction: row;
        height: 8px;
        border-radius: 4px;
        background-color: #1d2432;
     }
     .zgui-devtools__fill { height: 8px; background-color: #3aa76d }
     .zgui-devtools__fill-pinned { background-color: #8fdcb2 }
     .zgui-devtools__fill-over { background-color: #d1495b }
     .zgui-devtools__chip {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: 4px;
        padding: 1px 6px;
        border-radius: 4px;
        background-color: #1d2432;
        color: #9fb0c9;
     }
     .zgui-devtools__chip-on { background-color: #2f6bff; color: #ffffff }
     /* The tree above, what is picked below. The tree keeps the room the detail does not take,
        and the detail is capped rather than sized: what it holds is a computed style listing of a
        few hundred rows, and a pane that grew to fit one would be the whole panel. */
     .zgui-devtools__split {
        flex-direction: column;
        flex-grow: 1;
        min-height: 0;
     }
     .zgui-devtools__split-tree {
        flex-grow: 1;
        flex-shrink: 1;
        min-height: 0;
     }
     .zgui-devtools__split-detail {
        flex-grow: 0;
        flex-shrink: 0;
        min-height: 0;
     }
     .zgui-devtools__split-rule {
        display: flex;
        flex-direction: column;
        height: 7px;
        flex-grow: 0;
        flex-shrink: 0;
        align-self: stretch;
        justify-content: center;
        background-color: transparent;
     }
     .zgui-devtools__split-line {
        height: 1px;
        align-self: stretch;
        background-color: #2a3040;
     }
     .zgui-devtools__split-rule:hover .zgui-devtools__split-line { background-color: #2f6bff }
     .zgui-devtools__tree-row {
        flex-direction: row;
        align-items: center;
        gap: 4px;
        border-radius: 3px;
        flex-grow: 0;
        flex-shrink: 0;
        /* A deep row is indented a long way and still must not widen the panel: what does not fit
           is cut off, rather than pushing the whole tab sideways under a horizontal scrollbar. */
        min-width: 0;
        overflow: hidden;
     }
     .zgui-devtools__tree-row:hover { background-color: #1d2432 }
     .zgui-devtools__tree-picked { background-color: #23345c }
     .zgui-devtools__tree-chevron {
        display: flex;
        width: 12px;
        height: 12px;
        flex-grow: 0;
        flex-shrink: 0;
        align-items: center;
        color: #71809a;
     }
     .zgui-devtools__tree-arrow {
        display: block;
        flex-grow: 0;
        flex-shrink: 0;
        width: 12px;
        height: 12px;
        --zgui-fill: transparent;
        --zgui-stroke: #71809a;
        --zgui-stroke-width: 2px;
     }
     .zgui-devtools__tree-name { color: #e8edf6; flex-grow: 0; flex-shrink: 0 }
     .zgui-devtools__tree-component { color: #7ee3ff }
     .zgui-devtools__tree-id { color: #e0a83c }
     .zgui-devtools__tree-class { color: #8fdcb2 }
     .zgui-devtools__tree-text { color: #97a3b6 }
     .zgui-devtools__tree-source {
        color: #5c6a80;
        flex-grow: 1;
        flex-shrink: 1;
        min-width: 0;
        text-align: right;
     }
     .zgui-devtools-highlight {
        position: absolute;
        border: 1px solid #7ee3ff;
        background-color: rgba(47, 107, 255, 0.18);
        z-index: 2147482000;
        pointer-events: none;
     }
     .zgui-devtools-flash {
        position: absolute;
        border: 1px solid rgba(212, 73, 91, 0.9);
        background-color: rgba(212, 73, 91, 0.12);
        z-index: 2147481000;
        pointer-events: none;
     }"
);
