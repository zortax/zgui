//! The control that opens one section's panel.

use zgui::prelude::*;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::CHEVRON_DOWN;
use zgui_ui_primitives::use_roving_item;

use crate::navigation_menu::SHEET;
use crate::navigation_menu::item::NavigationMenuItemContext;
use crate::navigation_menu::style::NavigationMenuStyle;
use crate::support::activate_on_press;

/// The control that opens one section's panel.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     NavigationMenu {NavigationMenuList {NavigationMenuItem(value = "products") {
///         NavigationMenuTrigger {"Products"}
///         NavigationMenuContent {NavigationMenuLink {"Editor"}}
///     }}}
/// }
/// # }
/// ```
///
/// A button that says whether it is expanded and which panel it controls, rather than a menu item:
/// what opens is a panel of links, and a reader told "menu" would expect a keyboard model this is
/// not.
///
/// The chevron turns over as the panel arrives, and is hidden from a reader — it says the same
/// thing the expanded state already says.
#[component]
pub fn NavigationMenuTrigger(
    /// Classes merged after the trigger's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the trigger reads as.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, NavigationMenuStyle::CSS);
    let section = NavigationMenuItemContext::current();
    let node = section
        .as_ref()
        .map_or_else(NodeRef::new, NavigationMenuItemContext::trigger);
    let item = use_roving_item(node);

    let open = {
        let section = section.clone();
        move || {
            section
                .as_ref()
                .is_some_and(NavigationMenuItemContext::is_open)
        }
    };
    let mut semantics = A11yBinding::new(Role::Button).expanded(open.clone());
    if let Some(section) = &section {
        semantics = semantics.controls(section.content());
    }

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-state"), {
            let open = open.clone();
            move || Some(if open() { "open" } else { "closed" }.to_owned())
        })
        .a11y_from(semantics);

    let on_click = {
        let section = section.clone();
        move |_: &mut EventCx<'_, events::Click>| {
            if let Some(section) = &section {
                section.toggle();
            }
        }
    };
    view! {
        control(
            class = "zui-navigation-menu__trigger",
            node_ref = node,
            tabindex = move || item.map_or(Focus::Sequential, |item| item.tabindex().get()),
            on:focus_in = move |_| { if let Some(item) = item { item.activate() } },
            on:pointer_down = activate_on_press(),
            on:click = on_click,
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
            Icon(icon = CHEVRON_DOWN, class = "zui-navigation-menu__chevron")
        }
    }
}
