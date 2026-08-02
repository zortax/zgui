//! The list nested under one entry.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::vocab::{AriaCurrent, UiState};
use zgui::{component, view};

use crate::sidebar::shape::SidebarSubSize;
use crate::sidebar::style;

/// The list nested under one [`SidebarMenuItem`](crate::SidebarMenuItem).
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
///             SidebarMenuButton {"Settings"}
///             SidebarMenuSub {
///                 SidebarMenuSubItem {SidebarMenuSubButton(active = true) {"General"}}
///                 SidebarMenuSubItem {SidebarMenuSubButton {"Members"}}
///             }
///         }
///     }}}}}
/// }
/// # }
/// ```
///
/// The indent is a rule down its left edge rather than empty space: the eye follows the line back
/// to the entry the nested places belong to. Folded to icons the whole list goes — a rule down the
/// side of a 32px square would have nothing left of it to hold.
#[component]
pub fn SidebarMenuSub(
    /// Classes merged after the list's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The nested entries.
    children: Children,
) -> impl IntoView {
    style::install();
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::List));

    view! {
        column(class = "zui-sidebar__menu-sub", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// One entry of a [`SidebarMenuSub`].
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {Sidebar {SidebarContent {SidebarGroup {SidebarMenu {
///         SidebarMenuItem {SidebarMenuSub {
///             SidebarMenuSubItem {SidebarMenuSubButton {"Members"}}
///         }}
///     }}}}}
/// }
/// # }
/// ```
#[component]
pub fn SidebarMenuSubItem(
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
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::ListItem));

    view! {
        box(class = "zui-sidebar__menu-sub-item", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// The control inside a [`SidebarMenuSubItem`].
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {Sidebar {SidebarContent {SidebarGroup {SidebarMenu {
///         SidebarMenuItem {SidebarMenuSub {SidebarMenuSubItem {
///             SidebarMenuSubButton(active = true, on:click = move |_| ()) {"General"}
///         }}}
///     }}}}}
/// }
/// # }
/// ```
///
/// Shorter and quieter than the entry above it, and drawn without the weight change a current
/// top-level entry takes: a nested list is already indented, and a second signal saying the same
/// thing makes the column look ragged.
#[component]
pub fn SidebarMenuSubButton(
    /// Whether this entry is the place being shown.
    #[prop(into, default = Signal::stored_local(false))]
    active: Signal<bool, LocalStorage>,
    /// Whether it can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// How tall it is.
    #[prop(default = SidebarSubSize::Md)]
    size: SidebarSubSize,
    /// What it is called, for a reader, when what it holds does not say.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after the control's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the entry reads as.
    children: Children,
) -> impl IntoView {
    style::install();
    let mut semantics = A11yBinding::new(Role::Link)
        .disabled(move || disabled.get())
        .current(move || {
            if active.get() {
                AriaCurrent::Page
            } else {
                AriaCurrent::False
            }
        });
    if let Some(text) = label {
        semantics = semantics.label(text);
    }

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-active"), move || {
            active.get().then(|| "true".to_owned())
        })
        .attribute(zgui::view::AttrName::new("data-size"), size.name())
        .state(UiState::CHECKED, move || active.get())
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from(semantics);

    view! {
        control(
            class = "zui-sidebar__menu-sub-button",
            tabindex = {Focus::Sequential},
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}
