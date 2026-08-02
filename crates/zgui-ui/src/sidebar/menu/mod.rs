//! The list of places a sidebar goes.

mod action;
mod button;
mod skeleton;
mod state;
mod sub;

pub use crate::sidebar::menu::action::{
    SidebarMenuAction, SidebarMenuActionProps, SidebarMenuBadge, SidebarMenuBadgeProps,
};
pub use crate::sidebar::menu::button::{SidebarMenuButton, SidebarMenuButtonProps};
pub use crate::sidebar::menu::skeleton::{SidebarMenuSkeleton, SidebarMenuSkeletonProps};
pub use crate::sidebar::menu::state::SidebarMenuItemState;
pub use crate::sidebar::menu::sub::{
    SidebarMenuSub, SidebarMenuSubButton, SidebarMenuSubButtonProps, SidebarMenuSubItem,
    SidebarMenuSubItemProps, SidebarMenuSubProps,
};

use zgui::prelude::*;
use zgui::{component, view};

use crate::sidebar::style;

/// The list of entries inside a [`SidebarGroupContent`](crate::SidebarGroupContent).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {Sidebar {SidebarContent {SidebarGroup {SidebarMenu {
///         SidebarMenuItem {SidebarMenuButton {"Files"}}
///     }}}}}
/// }
/// # }
/// ```
///
/// A list, not a menu: the entries are places to go, they are all reachable by <kbd>Tab</kbd>, and
/// a reader told "menu" would expect a keyboard model this is deliberately not.
#[component]
pub fn SidebarMenu(
    /// Classes merged after the list's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The entries.
    children: Children,
) -> impl IntoView {
    style::install();
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::List));

    view! {
        column(class = "zui-sidebar__menu", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// One entry of a [`SidebarMenu`], and everything hung off it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {Sidebar {SidebarContent {SidebarGroup {SidebarMenu {
///         SidebarMenuItem {
///             SidebarMenuButton {"Inbox"}
///             SidebarMenuBadge {"24"}
///         }
///     }}}}}
/// }
/// # }
/// ```
///
/// The entry is what its parts are placed against: a [`SidebarMenuAction`] or a
/// [`SidebarMenuBadge`] stands at its right edge, at a height that depends on how tall the control
/// beside them is. Hanging either one on tells the control to leave room, so a long label ends in
/// an ellipsis before it reaches them rather than underneath them.
#[component]
pub fn SidebarMenuItem(
    /// Classes merged after the entry's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The control, and whatever hangs off it.
    children: Children,
) -> impl IntoView {
    style::install();
    let state = SidebarMenuItemState::new();
    provide_local_context(state);

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-action"), move || {
            state.is_crowded().then(|| "true".to_owned())
        })
        .attribute(zgui::view::AttrName::new("data-active"), move || {
            state.is_active().then(|| "true".to_owned())
        })
        .a11y_from(A11yBinding::new(Role::ListItem));

    view! {
        box(class = "zui-sidebar__menu-item", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
