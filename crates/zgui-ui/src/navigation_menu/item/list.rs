//! The bar the sections of a navigation menu sit in.

use zgui::prelude::*;
use zgui::{component, view};
use zgui_ui_primitives::Orientation;
use zgui_ui_primitives::prelude::*;

use crate::navigation_menu::SHEET;
use crate::navigation_menu::style::NavigationMenuStyle;

/// The bar of sections inside a [`NavigationMenu`](crate::NavigationMenu).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     NavigationMenu {
///         NavigationMenuList {
///             NavigationMenuItem(value = "pricing") {
///                 NavigationMenuLink {"Pricing"}
///             }
///         }
///     }
/// }
/// # }
/// ```
#[component]
pub fn NavigationMenuList(
    /// Classes merged after the bar's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The sections.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, NavigationMenuStyle::CSS);
    let own = Attrs::new()
        .class_toggle(
            zgui::view::ClassName::new("zui-navigation-menu__list"),
            true,
        )
        .a11y_from(A11yBinding::new(Role::List));

    view! {
        RovingFocus(orientation = Orientation::Horizontal, class = class, {..own}, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
