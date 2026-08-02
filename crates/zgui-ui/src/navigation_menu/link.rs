//! One link of a navigation menu.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::vocab::{AriaCurrent, UiState};
use zgui::{component, view};

use crate::navigation_menu::SHEET;
use crate::navigation_menu::style::NavigationMenuStyle;

/// One link of a [`NavigationMenu`](crate::NavigationMenu), in a panel or on the bar.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     NavigationMenu {NavigationMenuList {NavigationMenuItem(value = "pricing") {
///         NavigationMenuLink(active = true, on:click = move |_| ()) {"Pricing"}
///     }}}
/// }
/// # }
/// ```
///
/// `active` is the page the user is on, and it is announced as such — the same thing
/// [`BreadcrumbPage`](crate::BreadcrumbPage) says, in the one place a navigation menu can say it.
/// A sheet picks the same link out through `data-active`.
///
/// There is no destination prop: what a link does is whatever its `on:click` does, and an
/// application that routes hands it the handler it hands every other control that navigates.
#[component]
pub fn NavigationMenuLink(
    /// Whether this link is the page being shown.
    #[prop(into, default = Signal::stored_local(false))]
    active: Signal<bool, LocalStorage>,
    /// Classes merged after the link's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the link reads as.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, NavigationMenuStyle::CSS);
    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-active"), move || {
            active.get().then(|| "true".to_owned())
        })
        .state(UiState::CHECKED, move || active.get())
        .a11y_from(A11yBinding::new(Role::Link).current(move || {
            if active.get() {
                AriaCurrent::Page
            } else {
                AriaCurrent::False
            }
        }));

    view! {
        control(
            class = "zui-navigation-menu__link",
            tabindex = {Focus::Sequential},
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}
