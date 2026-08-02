//! The arrow that points from the bar at the open panel.

use zgui::prelude::*;
use zgui::{component, view};

use crate::navigation_menu::SHEET;
use crate::navigation_menu::item::NavigationMenuItemContext;
use crate::navigation_menu::style::NavigationMenuStyle;

/// The arrow between a section's trigger and the panel it opened.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui::navigation_menu::{NavigationMenuIndicator, NavigationMenuIndicatorProps};
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     NavigationMenu {NavigationMenuList {NavigationMenuItem(value = "products") {
///         NavigationMenuTrigger {"Products"}
///         NavigationMenuIndicator()
///         NavigationMenuContent {NavigationMenuLink {"Editor"}}
///     }}}
/// }
/// # }
/// ```
///
/// Optional, and written inside the section it belongs to. It fades in with the panel rather than
/// sliding along the bar, because the panel it points at is placed against its own trigger — an
/// arrow that travelled would be a second animation of the same fact, arriving at a different time.
///
/// Hidden from a reader outright: it is punctuation between a control and what that control opened,
/// and the relation between those two is already stated.
#[component]
pub fn NavigationMenuIndicator(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, NavigationMenuStyle::CSS);
    let section = NavigationMenuItemContext::current();
    let open = move || {
        section
            .as_ref()
            .is_some_and(NavigationMenuItemContext::is_open)
    };

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(if open() { "visible" } else { "hidden" }.to_owned())
        })
        .a11y_from(A11yBinding::unspecified().hidden(true));

    view! {
        box(class = "zui-navigation-menu__indicator", {..own}, {..attrs}, class = class) {
            box(class = "zui-navigation-menu__indicator-arrow")
        }
    }
}
