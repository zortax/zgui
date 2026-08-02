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

/// The inspector's style sheet, to install beside the application's own.
///
/// ```no_run
/// use zgui::prelude::*;
///
/// # fn main() -> Result<(), zgui::Error> {
/// app()
///     .with_stylesheet(format!("{}\n{}", "root { padding: 0 }", zgui_devtools::SHEET))
///     .run(|| view! { column() })
/// # }
/// ```
pub const SHEET: &str = zgui::css!(
    ".zgui-devtools-host { flex-grow: 1; align-self: stretch; flex-direction: column }
     .zgui-devtools-host-docked {
        flex-direction: row;
        align-items: stretch;
        width: 100%;
        height: 100%;
     }
     .zgui-devtools-app { flex-direction: column; flex-grow: 1; flex-shrink: 1; min-width: 0 }
     .zgui-devtools-app-docked { overflow: auto }
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
        gap: 2px;
        padding: 6px;
        border-bottom: 1px solid #2a3040;
        background-color: #151924;
     }
     .zgui-devtools__tab {
        padding: 4px 9px;
        border-radius: 5px;
        color: #97a3b6;
        background-color: transparent;
     }
     .zgui-devtools__tab:hover { background-color: #1d2432 }
     .zgui-devtools__tab-on { background-color: #2f6bff; color: #ffffff }
     .zgui-devtools__spacer { flex-grow: 1 }
     .zgui-devtools__toggle {
        padding: 4px 8px;
        border-radius: 5px;
        border: 1px solid #2a3040;
        color: #97a3b6;
     }
     .zgui-devtools__toggle-on { background-color: #b45309; color: #ffffff }
     .zgui-devtools__body {
        flex-direction: column;
        gap: 10px;
        padding: 10px;
        overflow-y: auto;
        flex-grow: 1;
     }
     .zgui-devtools__head { color: #7ee3ff; padding-bottom: 2px }
     .zgui-devtools__row {
        flex-direction: row;
        gap: 8px;
        align-items: baseline;
     }
     .zgui-devtools__key { width: 168px; color: #8b97ab }
     .zgui-devtools__value { flex-grow: 1; color: #e8edf6 }
     .zgui-devtools__value-quiet { flex-grow: 1; color: #71809a }
     .zgui-devtools__note { color: #71809a }
     .zgui-devtools__box {
        flex-direction: column;
        gap: 2px;
        padding: 8px;
        border: 1px dashed #8b6f2f;
        background-color: #191712;
     }
     .zgui-devtools__box-border { border-color: #2f6bff; background-color: #101526 }
     .zgui-devtools__box-padding { border-color: #2f8f5b; background-color: #0f1a14 }
     .zgui-devtools__box-content { border-color: #7ee3ff; background-color: #0e1a1e }
     .zgui-devtools__strip { flex-direction: row; height: 16px; gap: 1px }
     .zgui-devtools__slice { background-color: #2f6bff; height: 16px }
     .zgui-devtools__slice-slow { background-color: #d1495b }
     .zgui-devtools__chip {
        padding: 1px 6px;
        border-radius: 4px;
        background-color: #1d2432;
        color: #9fb0c9;
     }
     .zgui-devtools-highlight {
        position: absolute;
        border: 1px solid #7ee3ff;
        background-color: rgba(47, 107, 255, 0.18);
        z-index: 2147482000;
     }
     .zgui-devtools-flash {
        position: absolute;
        border: 1px solid rgba(212, 73, 91, 0.9);
        background-color: rgba(212, 73, 91, 0.12);
        z-index: 2147481000;
     }"
);
