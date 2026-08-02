//! The name of a key, set as a key.

mod style;

pub use crate::kbd::style::KbdStyle;

use zgui::prelude::*;
use zgui::{component, view};

/// What the key's rules are installed under.
const SHEET: &str = "zui-kbd";

/// One key, drawn as the keycap it names.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// The shortcut that opens the palette.
/// #[component]
/// fn Shortcut() -> impl IntoView {
///     view! { KbdGroup {Kbd {"Ctrl"} Kbd {"K"}} }
/// }
/// ```
///
/// # It is not a control
///
/// A keycap is a piece of writing that happens to look like a button. It takes no pointer events
/// and no selection, so a drag across a menu item does not stop on the shortcut printed at its
/// end and a click passes through to whatever is behind.
#[component]
pub fn Kbd(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The key's name.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, KbdStyle::CSS);
    view! {
        box(class = KbdStyle::CLASS, {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// Several [`Kbd`] keys pressed together or in turn.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { KbdGroup {Kbd {"Ctrl"} Kbd {"Shift"} Kbd {"P"}} }
/// # }
/// ```
#[component]
pub fn KbdGroup(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The keys, and whatever joins them.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, KbdStyle::CSS);
    view! {
        box(class = "zui-kbd-group", {..attrs}, class = class) {{children.into_view_once()}}
    }
}
