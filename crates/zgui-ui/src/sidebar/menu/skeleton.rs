//! An entry that is still being fetched.

use core::cell::Cell;

use zgui::prelude::*;
use zgui::view::CustomPropertyName;
use zgui::{component, view};

use crate::sidebar::style;
use crate::skeleton::SkeletonProps;

/// The widths a run of waiting entries cycles through, as a fraction of the entry.
///
/// A column of identical bars reads as a table rather than as a list of names, so the widths vary —
/// but they vary by rule rather than by chance, so the same panel draws the same way twice.
const WIDTHS: [u8; 5] = [72, 54, 86, 63, 78];

thread_local! {
    /// Which width the next waiting entry takes.
    static NEXT: Cell<usize> = const { Cell::new(0) };
}

/// A [`SidebarMenuItem`](crate::SidebarMenuItem) standing in for one still being fetched.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {Sidebar {SidebarContent {SidebarGroup {SidebarMenu {
///         for index in move || 0..3, key = |index: &i32| *index {
///             SidebarMenuItem {SidebarMenuSkeleton(icon = true)}
///         }
///     }}}}}
/// }
/// # }
/// ```
///
/// Exactly the height and the padding of the entry it stands in for, so the list does not jump when
/// the names arrive.
#[component]
pub fn SidebarMenuSkeleton(
    /// Whether it leaves room for the mark the entry will lead with.
    #[prop(default = false)]
    icon: bool,
    /// How wide the bar standing in for the name is, as a percentage of the entry.
    #[prop(optional)]
    width: Option<u8>,
    /// Classes merged after the entry's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    style::install();
    let width = width.unwrap_or_else(|| {
        NEXT.with(|next| {
            let taken = next.get();
            next.set((taken + 1) % WIDTHS.len());
            WIDTHS[taken]
        })
    });
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::GenericContainer).busy(true));
    let bar = Attrs::new().custom_property(
        CustomPropertyName::new("zui-sidebar-skeleton-width"),
        move || Some(format!("{width}%")),
    );

    view! {
        box(class = "zui-sidebar__menu-skeleton", {..own}, {..attrs}, class = class) {
            if move || icon {
                Skeleton(class = "zui-sidebar__menu-skeleton-icon")
            }
            Skeleton(class = "zui-sidebar__menu-skeleton-text", {..bar.clone()})
        }
    }
}
