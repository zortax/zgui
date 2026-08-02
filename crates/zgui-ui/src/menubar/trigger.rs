//! The name of a menu on the bar.

use zgui::prelude::*;
use zgui::vocab::{HasPopup, Key, NamedKey};
use zgui::{component, view};
use zgui_ui_primitives::use_roving_item;

use crate::menubar::style::MenubarStyle;
use crate::menubar::{MenubarMenuContext, SHEET};

/// The name of a [`MenubarMenu`](crate::MenubarMenu), as it appears on the bar.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Menubar {MenubarMenu(value = "file") {
///         MenubarTrigger {"File"}
///         MenubarContent {MenubarItem {"New"}}
///     }}
/// }
/// # }
/// ```
///
/// # Moving along an open bar
///
/// Moving the focus — or the pointer — onto a name while some menu is already open opens *this*
/// one, which is what makes a bar a bar. With nothing open, arriving opens nothing: arrowing or
/// mousing along a closed bar is a survey, not a series of menus flashing past.
#[component]
pub fn MenubarTrigger(
    /// Classes merged after the name's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the name reads as.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, MenubarStyle::CSS);
    let menu = MenubarMenuContext::current();
    let node = menu
        .as_ref()
        .map_or_else(NodeRef::new, MenubarMenuContext::trigger);
    let item = use_roving_item(node);

    let open = {
        let menu = menu.clone();
        move || menu.as_ref().is_some_and(MenubarMenuContext::is_open)
    };

    let mut semantics = A11yBinding::new(Role::MenuItem)
        .has_popup(HasPopup::Menu)
        .expanded(open.clone());
    if let Some(menu) = &menu {
        semantics = semantics.controls(menu.content());
    }

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-state"), {
            let open = open.clone();
            move || Some(if open() { "open" } else { "closed" }.to_owned())
        })
        .a11y_from(semantics);

    let on_focus = {
        let menu = menu.clone();
        move |_: &mut EventCx<'_, events::FocusIn>| {
            if let Some(item) = item {
                item.activate();
            }
            // Only when a menu is already open. Otherwise walking the bar with the arrow keys
            // would open every menu it passes.
            if let Some(menu) = &menu
                && menu.bar_has_something_open()
            {
                menu.open();
            }
        }
    };
    let on_pointer_enter = {
        let menu = menu.clone();
        move |_: &mut EventCx<'_, events::PointerEnter>| {
            // The pointer follows the same rule the focus does, because an open bar is one
            // gesture: click "File", slide to "Edit", and Edit's menu is up without a second
            // press. With nothing open, hover opens nothing — passing the pointer along a closed
            // bar is on the way to somewhere else, not a request for every menu it crosses.
            if let Some(menu) = &menu
                && menu.bar_has_something_open()
                && !menu.is_open()
            {
                if let Some(item) = item {
                    item.activate();
                }
                menu.open();
            }
        }
    };
    let on_click = {
        let menu = menu.clone();
        move |_: &mut EventCx<'_, events::Click>| {
            if let Some(menu) = &menu {
                menu.toggle();
            }
        }
    };
    let on_key = {
        let menu = menu.clone();
        move |ev: &mut EventCx<'_, events::KeyDown>| {
            let Some(menu) = &menu else { return };
            if matches!(ev.key, Key::Named(NamedKey::ArrowDown)) {
                menu.open();
                ev.prevent_default();
                ev.stop_propagation();
            }
        }
    };

    view! {
        control(
            class = "zui-menubar__trigger",
            node_ref = node,
            tabindex = move || item.map_or(Focus::Sequential, |item| item.tabindex().get()),
            on:focus_in = on_focus,
            on:pointer_enter = on_pointer_enter,
            on:click = on_click,
            on:key_down = on_key,
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}
