//! The list of pages down the leading edge.

use zgui::prelude::*;
use zgui::{component, view};
use zgui_ui_primitives::prelude::*;

use crate::settings::style;

/// The list of pages of a [`Settings`](crate::Settings).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Settings(default_page = "appearance") {
///         SettingsPages {SettingsPage(value = "appearance") {"Appearance"}}
///         SettingsPane(value = "appearance") {text {"Colours and type."}}
///     }
/// }
/// # }
/// ```
///
/// # Keyboard
///
/// One tab stop for the whole list, which is [`RovingFocus`](zgui_ui_primitives::RovingFocus)
/// doing the work — the same primitive a toolbar and a tab strip are built on.
/// <kbd>↑</kbd> and <kbd>↓</kbd> move between the pages and wrap at the ends, <kbd>Home</kbd> and
/// <kbd>End</kbd> go to the first and last, and <kbd>Tab</kbd> leaves the list for the pane. The
/// horizontal arrows are left alone, so the list does not swallow the keys that move a caret in
/// the pane beside it.
#[component]
pub fn SettingsPages(
    /// What the list is called, for a reader, when the settings themselves are not named.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The entries.
    children: Children,
) -> impl IntoView {
    style::install();
    let mut semantics =
        A11yBinding::new(Role::TabList).orientation(zgui::vocab::Orientation::Vertical);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-settings__pages"), true)
        .a11y_from(semantics);

    view! {
        RovingFocus(orientation = Orientation::Vertical, class = class, {..own}, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
