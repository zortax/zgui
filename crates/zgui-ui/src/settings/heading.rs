//! The writing over a group of settings.

use zgui::prelude::*;
use zgui::{component, view};

use crate::settings::context::SettingsGroupContext;
use crate::settings::style;

/// The heading over a [`SettingsGroup`](crate::SettingsGroup).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { SettingsGroup {SettingsGroupLabel {"Theme"}} }
/// # }
/// ```
///
/// It names the group without being told which element that is: the group published the handle, so
/// a caller cannot wire the two together wrongly and cannot forget to wire them at all.
#[component]
pub fn SettingsGroupLabel(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the heading says.
    children: Children,
) -> impl IntoView {
    style::install();
    let node = SettingsGroupContext::current().map_or_else(NodeRef::new, |group| group.label);

    view! {
        label(class = "zui-settings__group-label", node_ref = node, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// A line under a [`SettingsGroupLabel`] qualifying it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SettingsGroup {
///         SettingsGroupLabel {"Gateway API"}
///         SettingsGroupDescription {"Resources the specification has not settled."}
///     }
/// }
/// # }
/// ```
#[component]
pub fn SettingsGroupDescription(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    style::install();
    let node = SettingsGroupContext::current().map_or_else(NodeRef::new, |group| group.description);

    view! {
        box(
            class = "zui-settings__group-description",
            node_ref = node,
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}
