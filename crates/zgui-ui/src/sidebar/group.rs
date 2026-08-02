//! One run of related places inside the scrolling band, and what heads it.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};

use crate::sidebar::style;

/// One run of related entries inside a [`SidebarContent`](crate::SidebarContent).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {Sidebar {SidebarContent {SidebarGroup {
///         SidebarGroupLabel {"Views"}
///         SidebarGroupContent {SidebarMenu {
///             SidebarMenuItem {SidebarMenuButton {"Files"}}
///         }}
///     }}}}
/// }
/// # }
/// ```
///
/// A group with a [`SidebarGroupLabel`] is named by it, so a reader meets "Views, group" rather
/// than a heading followed by an anonymous box. The group is also what a
/// [`SidebarGroupAction`] is placed against, which is why it establishes a coordinate system of
/// its own.
#[component]
pub fn SidebarGroup(
    /// Classes merged after the group's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The heading and the entries.
    children: Children,
) -> impl IntoView {
    style::install();
    let heading = NodeRef::new();
    provide_local_context(SidebarGroupHeading(heading));
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::Group).labelled_by(heading));

    view! {
        column(class = "zui-sidebar__group", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// Where a group records the element that names it.
#[derive(Copy, Clone)]
struct SidebarGroupHeading(NodeRef);

/// The heading over a [`SidebarGroup`].
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {Sidebar {SidebarContent {SidebarGroup {
///         SidebarGroupLabel {"Views"}
///     }}}}
/// }
/// # }
/// ```
///
/// Folding the panel to icons does not simply fade the heading out and leave a hole where it was:
/// it pulls the heading up by its own height at the same time, so the entries below close over it
/// as it goes.
#[component]
pub fn SidebarGroupLabel(
    /// Classes merged after the heading's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the heading says.
    children: Children,
) -> impl IntoView {
    style::install();
    let node = use_local_context::<SidebarGroupHeading>().map_or_else(NodeRef::new, |it| it.0);

    view! {
        label(class = "zui-sidebar__group-label", node_ref = node, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// A control at the right end of a [`SidebarGroup`]'s heading row.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui_icons::prelude::*;
/// # use zgui_ui_icons::set::mark::PLUS;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {Sidebar {SidebarContent {SidebarGroup {
///         SidebarGroupLabel {"Projects"}
///         SidebarGroupAction(label = "Add a project", on:click = move |_| ()) {
///             Icon(icon = PLUS, size = IconSize::Sm)
///         }
///     }}}}
/// }
/// # }
/// ```
///
/// Placed against the group rather than laid out in its heading row, so a heading long enough to
/// wrap runs under it instead of pushing it off. It goes when the panel folds to icons, with the
/// heading it belongs to.
#[component]
pub fn SidebarGroupAction(
    /// What the control is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Whether it can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Classes merged after the control's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The mark it carries.
    children: Children,
) -> impl IntoView {
    style::install();
    let mut semantics = A11yBinding::new(Role::Button).disabled(move || disabled.get());
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    let own = Attrs::new().a11y_from(semantics);

    view! {
        control(
            class = "zui-sidebar__group-action",
            tabindex = {Focus::Sequential},
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}

/// What a [`SidebarGroup`] holds under its heading.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {Sidebar {SidebarContent {SidebarGroup {
///         SidebarGroupContent {SidebarMenu {
///             SidebarMenuItem {SidebarMenuButton {"Files"}}
///         }}
///     }}}}
/// }
/// # }
/// ```
///
/// Where the panel settles on its reading size, so a group holding a list and a group holding a
/// paragraph are set in the same type.
#[component]
pub fn SidebarGroupContent(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The entries, or whatever else the group holds.
    children: Children,
) -> impl IntoView {
    style::install();
    view! {
        box(class = "zui-sidebar__group-content", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
