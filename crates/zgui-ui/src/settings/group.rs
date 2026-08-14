//! One run of related settings inside a pane.

use zgui::prelude::*;
use zgui::{component, view};

use crate::settings::context::SettingsGroupContext;
use crate::settings::style;

/// One run of related settings inside a [`SettingsPane`](crate::SettingsPane).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SettingsGroup {
///         SettingsGroupLabel {"Log streams"}
///         SettingsGroupDescription {"How a stream behaves when the connection drops."}
///         SettingsItem(label = "Reconnect attempts") {Input()}
///     }
/// }
/// # }
/// ```
///
/// A group with a [`SettingsGroupLabel`](crate::SettingsGroupLabel) is named by it, so a reader
/// meets "Log streams, group" rather than a heading followed by an anonymous box.
#[component]
pub fn SettingsGroup(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The heading, and the settings under it.
    children: Children,
) -> impl IntoView {
    style::install();
    let context = SettingsGroupContext {
        label: NodeRef::new(),
        description: NodeRef::new(),
    };
    provide_local_context(context);

    let own = Attrs::new().a11y_from(
        A11yBinding::new(Role::Group)
            .labelled_by(context.label)
            .described_by(context.description),
    );

    view! {
        column(class = "zui-settings__group", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
