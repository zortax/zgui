//! Menubar items that carry a setting rather than a command.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::view::{AttrName, ClassName};
use zgui::vocab::UiState;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::mark::{CHECK, DOT};
use zgui_ui_primitives::{Binding, Controllable, use_roving_item};

use crate::menu::MenuRadioContext;
use crate::menu::defocus_on_leave;
use crate::menubar::style::MenubarStyle;
use crate::menubar::{MenubarMenuContext, SHEET};

/// A menubar item that is on or off, with a tick where a symbol would go.
///
/// The tick keeps its column whether it is showing or not, so a run of settings and the commands
/// among them all start their labels in the same place.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
/// use zgui_ui::menubar::{MenubarCheckboxItem, MenubarCheckboxItemProps};
///
/// /// What the view shows, kept on the bar.
/// #[component]
/// fn ViewMenu() -> impl IntoView {
///     let gutters = RwSignal::new_local(true);
///     view! {
///         Menubar {MenubarMenu(value = "view") {
///             MenubarTrigger {"View"}
///             MenubarContent {
///                 MenubarCheckboxItem(checked = gutters) {"Show gutters"}
///             }
///         }}
///     }
/// }
/// ```
#[component]
pub fn MenubarCheckboxItem(
    /// Whether it is ticked, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    checked: Binding<bool>,
    /// Whether it starts ticked, when the item owns that itself.
    #[prop(default = false)]
    default_checked: bool,
    /// Told whenever it is ticked or unticked, whoever owns it.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<bool>>,
    /// Whether it can be chosen.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Whether choosing it closes the menu.
    ///
    /// It does not, unlike a command: a setting is the one thing a user is likely to want to
    /// change twice, and a menu that shut on the first change would have to be re-opened for it.
    #[prop(default = false)]
    close_on_select: bool,
    /// Classes merged after the item's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, MenubarStyle::CSS);
    let node = NodeRef::new();
    let item = use_roving_item(node);
    let menu = MenubarMenuContext::current();
    let on_leave = defocus_on_leave(node, menu.as_ref().map(MenubarMenuContext::content));
    let value = Controllable::new(checked, default_checked, on_change);

    let own = Attrs::new()
        .class_toggle(ClassName::new("zui-menubar__item"), true)
        .class_toggle(ClassName::new("zui-menubar__item--check"), true)
        .attribute(AttrName::new("data-state"), move || {
            Some(if value.get() { "checked" } else { "unchecked" }.to_owned())
        })
        .state(UiState::CHECKED, move || value.get())
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from(
            A11yBinding::new(Role::MenuItemCheckBox)
                .toggled_on(move || value.get())
                .disabled(move || disabled.get()),
        );

    view! {
        control(
            node_ref = node,
            tabindex = move || item.map_or(Focus::Sequential, |item| item.tabindex().get()),
            on:focus_in = move |_| { if let Some(item) = item { item.activate() } },
            // The pointer moves the keyboard rather than shadowing it, exactly as in a menu: one
            // highlight, arriving with the pointer and leaving with it.
            on:pointer_enter = move |_| {
                if !disabled.get_untracked() {
                    if let Some(item) = item {
                        item.activate();
                    }
                    node.focus();
                }
            },
            on:pointer_leave = on_leave,
            on:click = move |_| {
                if disabled.get_untracked() {
                    return;
                }
                value.toggle();
                if close_on_select && let Some(menu) = &menu {
                    menu.dismiss();
                }
            },
            {..own},
            {..attrs},
            class = class
        ) {
            box(class = "zui-menubar__indicator") {
                if move || value.get() {
                    Icon(icon = CHECK)
                } else {}
            }
            box(class = "zui-menubar__item-label") {{children.into_view_once()}}
        }
    }
}

/// One of a run of menubar items of which exactly one is chosen.
///
/// It finds which one that is from the enclosing
/// [`MenubarRadioGroup`](crate::menubar::MenubarRadioGroup), so nothing is threaded between them.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
/// use zgui_ui::menubar::{MenubarRadioItem, MenubarRadioItemProps};
///
/// /// Which pane the window is showing.
/// #[component]
/// fn LayoutMenu() -> impl IntoView {
///     view! {
///         Menubar {MenubarMenu(value = "layout") {
///             MenubarTrigger {"Layout"}
///             MenubarContent {
///                 MenuRadioGroup(default_value = "split") {
///                     MenubarRadioItem(value = "single") {"One pane"}
///                     MenubarRadioItem(value = "split") {"Two panes"}
///                 }
///             }
///         }}
///     }
/// }
/// ```
#[component]
pub fn MenubarRadioItem(
    /// What choosing it reports, and what it is known by inside the group.
    #[prop(into)]
    value: String,
    /// Whether it can be chosen.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Whether choosing it closes the menu.
    #[prop(default = true)]
    close_on_select: bool,
    /// Classes merged after the item's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, MenubarStyle::CSS);
    let node = NodeRef::new();
    let item = use_roving_item(node);
    let menu = MenubarMenuContext::current();
    let on_leave = defocus_on_leave(node, menu.as_ref().map(MenubarMenuContext::content));
    let group = MenuRadioContext::current();

    let chosen = {
        let value = value.clone();
        move || group.is_some_and(|group| group.is_chosen(&value))
    };
    let choose = {
        let value = value.clone();
        move || {
            if disabled.get_untracked() {
                return;
            }
            if let Some(group) = group {
                group.choose(&value);
            }
            if close_on_select && let Some(menu) = &menu {
                menu.dismiss();
            }
        }
    };

    let own = Attrs::new()
        .class_toggle(ClassName::new("zui-menubar__item"), true)
        .class_toggle(ClassName::new("zui-menubar__item--check"), true)
        .attribute(AttrName::new("data-value"), {
            let value = value.clone();
            move || Some(value.clone())
        })
        .state(UiState::CHECKED, chosen.clone())
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from(
            A11yBinding::new(Role::MenuItemRadio)
                .toggled_on(chosen.clone())
                .disabled(move || disabled.get()),
        );

    view! {
        control(
            node_ref = node,
            tabindex = move || item.map_or(Focus::Sequential, |item| item.tabindex().get()),
            on:focus_in = move |_| { if let Some(item) = item { item.activate() } },
            // The pointer moves the keyboard rather than shadowing it, exactly as in a menu: one
            // highlight, arriving with the pointer and leaving with it.
            on:pointer_enter = move |_| {
                if !disabled.get_untracked() {
                    if let Some(item) = item {
                        item.activate();
                    }
                    node.focus();
                }
            },
            on:pointer_leave = on_leave,
            on:click = move |_| choose(),
            {..own},
            {..attrs},
            class = class
        ) {
            box(class = "zui-menubar__indicator", class = "zui-menubar__indicator--dot") {
                Show(when = chosen.clone(), fallback = || ()) {
                    Icon(icon = DOT)
                }
            }
            box(class = "zui-menubar__item-label") {{children.into_view_once()}}
        }
    }
}
