//! The two columns a page of settings is made of.

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::{component, view};
use zgui_ui_primitives::Binding;

use crate::settings::context::SettingsContext;
use crate::settings::style::{self, SettingsStyle};

/// A page of settings: the pages down one side, and what each one holds down the other.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Two pages of preferences that take effect as they are changed.
/// #[component]
/// fn Preferences() -> impl IntoView {
///     let dark = RwSignal::new_local(true);
///     let shell = RwSignal::new_local(String::new());
///
///     view! {
///         Settings(default_page = "appearance", label = "Preferences") {
///             SettingsPages {
///                 SettingsPage(value = "appearance") {"Appearance"}
///                 SettingsPage(value = "terminal") {"Terminal"}
///             }
///             SettingsPane(value = "appearance") {
///                 SettingsGroup {
///                     SettingsGroupLabel {"Theme"}
///                     SettingsItem(label = "Dark mode") {
///                         Switch(checked = dark, {..use_settings_item_attrs()})
///                     }
///                 }
///             }
///             SettingsPane(value = "terminal") {
///                 SettingsGroup {
///                     SettingsGroupLabel {"Shell"}
///                     SettingsItem(
///                         label = "Custom shell",
///                         description = "Left empty, the system shell is used."
///                     ) {
///                         Input(value = shell, {..use_settings_item_attrs()})
///                     }
///                 }
///             }
///         }
///     }
/// }
/// ```
///
/// # Who owns the page
///
/// The same three props every other component here takes: `page` for a caller who holds it,
/// `default_page` for one who does not, and `on_page_change` for anybody who wants to be told. An
/// application that restores the page it was last on writes its own signal to `page`; one that
/// opens on the first page every time writes `default_page` and nothing else.
///
/// # What a reader is told
///
/// That the list is a list of tabs and each pane is the panel one of them shows. Choosing a page
/// swaps the whole right-hand column, which is what a tab does — so the entries carry the
/// selection, name the pane they control, and answer the arrow keys as one tab stop.
#[component]
pub fn Settings(
    /// Which page is showing, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    page: Binding<String>,
    /// Which page starts showing, when the settings own that themselves.
    #[prop(into, default = String::new())]
    default_page: String,
    /// Told whenever the showing page changes, whoever owns it.
    #[prop(optional)]
    on_page_change: Option<UnsyncCallback<String>>,
    /// What the whole thing is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Where to record this component's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The page list and the panes.
    children: Children,
) -> impl IntoView {
    style::install();
    let element = node_ref.unwrap_or_default();
    let context = SettingsContext::new(page, default_page, on_page_change);
    provide_local_context(context);

    let mut semantics = A11yBinding::new(Role::Group);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-settings"), true)
        .attribute(zgui::view::AttrName::new("data-page"), move || {
            Some(context.page())
        })
        .a11y_from(semantics);

    view! {
        row(node_ref = element, class = SettingsStyle::CLASS, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
