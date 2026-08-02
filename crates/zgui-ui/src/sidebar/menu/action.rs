//! What stands at the right edge of an entry: a control to press, or a count to read.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};

use crate::sidebar::menu::state::SidebarMenuItemState;
use crate::sidebar::shape::SidebarMenuSize;
use crate::sidebar::style;

/// A small control at the right edge of a [`SidebarMenuItem`](crate::SidebarMenuItem).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui_icons::prelude::*;
/// # use zgui_ui_icons::set::ui::ELLIPSIS;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {Sidebar {SidebarContent {SidebarGroup {SidebarMenu {
///         SidebarMenuItem {
///             SidebarMenuButton {"Project"}
///             SidebarMenuAction(label = "More", hover_only = true, on:click = move |_| ()) {
///                 Icon(icon = ELLIPSIS, size = IconSize::Sm)
///             }
///         }
///     }}}}}
/// }
/// # }
/// ```
///
/// Placed against the entry rather than laid out in it, at the height its control asks for, so a
/// short entry and a tall one both put it where the eye expects. It goes with the labels when the
/// panel folds to icons: there is no room beside a 32px square for anything else.
///
/// `hover_only` keeps it out of sight until the pointer is over the entry or something in the
/// entry has focus — for the actions a crowded list should not show all at once.
#[component]
pub fn SidebarMenuAction(
    /// What the control is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Whether it can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Whether it stays out of sight until the entry is hovered or holds focus.
    #[prop(default = false)]
    hover_only: bool,
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
    let entry = SidebarMenuItemState::current();
    if let Some(entry) = entry {
        entry.crowd();
    }

    let mut semantics = A11yBinding::new(Role::Button).disabled(move || disabled.get());
    if let Some(text) = label {
        semantics = semantics.label(text);
    }

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-size"), move || {
            Some(
                entry
                    .map_or(SidebarMenuSize::Default, SidebarMenuItemState::size)
                    .name()
                    .to_owned(),
            )
        })
        .attribute(zgui::view::AttrName::new("data-hover-only"), move || {
            hover_only.then(|| "true".to_owned())
        })
        .a11y_from(semantics);

    view! {
        control(
            class = "zui-sidebar__menu-action",
            tabindex = {Focus::Sequential},
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}

/// A count at the right edge of a [`SidebarMenuItem`](crate::SidebarMenuItem).
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
/// A count, not a control: it is never pointed at and never selected, because reaching for it is
/// always a miss for the entry underneath. Its figures are tabular, so a column of counts lines up
/// digit under digit as they change.
#[component]
pub fn SidebarMenuBadge(
    /// Classes merged after the count's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The count.
    children: Children,
) -> impl IntoView {
    style::install();
    let entry = SidebarMenuItemState::current();
    if let Some(entry) = entry {
        entry.crowd();
    }

    let own = Attrs::new().attribute(zgui::view::AttrName::new("data-size"), move || {
        Some(
            entry
                .map_or(SidebarMenuSize::Default, SidebarMenuItemState::size)
                .name()
                .to_owned(),
        )
    });

    view! {
        box(class = "zui-sidebar__menu-badge", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
