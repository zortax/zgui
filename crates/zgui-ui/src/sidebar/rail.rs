//! The edge of the panel, as something to press.

use zgui::prelude::*;
use zgui::{component, view};

use crate::sidebar::context::SidebarContext;
use crate::sidebar::style;

/// The strip along the panel's outer edge that folds it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {Sidebar {
///         SidebarContent {text {"Files"}}
///         SidebarRail()
///     }}
/// }
/// # }
/// ```
///
/// It straddles the edge — half over the panel and half over the page — so the pointer meets it a
/// little before the edge and a little after, and it shows a line under the pointer at the place
/// the edge would be dragged from. The cursor points the way the panel would go.
///
/// # Not in the tab order
///
/// Deliberately: it does the same thing as the [`SidebarTrigger`](crate::SidebarTrigger), which is
/// in the tab order and says what it is. A second stop that read the same and looked like nothing
/// would be a keyboard user's tax on a pointer user's convenience. It is still reachable by name
/// for anything that goes looking.
#[component]
pub fn SidebarRail(
    /// What the strip is called, for a reader.
    #[prop(into, default = String::from("Toggle sidebar"))]
    label: String,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    style::install();
    let context = SidebarContext::current();
    let open = move || context.is_some_and(SidebarContext::is_open);

    let mut semantics = A11yBinding::new(Role::Button).label(label).expanded(open);
    if let Some(context) = context {
        semantics = semantics.controls(context.panel());
    }
    let own = Attrs::new().a11y_from(semantics);

    view! {
        control(
            class = "zui-sidebar__rail",
            tabindex = {Focus::Programmatic},
            on:click = move |_| { if let Some(context) = context { context.toggle() } },
            {..own},
            {..attrs},
            class = class
        )
    }
}
