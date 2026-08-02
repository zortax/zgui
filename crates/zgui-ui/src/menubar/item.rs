//! What is inside a menu: the things that can be chosen, and the things that cannot.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::vocab::UiState;
use zgui::{component, view};
use zgui_ui_primitives::use_roving_item;

use crate::menu::defocus_on_leave;
use crate::menubar::style::MenubarStyle;
use crate::menubar::{MenubarMenuContext, SHEET};

/// One thing that can be chosen from a [`MenubarContent`](crate::MenubarContent).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::reactive::UnsyncCallback;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Menubar {MenubarMenu(value = "file") {
///         MenubarTrigger {"File"}
///         MenubarContent {
///             MenubarItem(on_select = UnsyncCallback::new(|()| ()), shortcut = "Ctrl+N") {
///                 "New"
///             }
///         }
///     }}
/// }
/// # }
/// ```
///
/// Choosing an item closes the menu and puts the focus back on the name that opened it, which is
/// what a menu is for: it is a detour, and the keyboard has to come back from it.
#[component]
pub fn MenubarItem(
    /// Told when this item is chosen.
    #[prop(optional)]
    on_select: Option<UnsyncCallback<()>>,
    /// Whether it can be chosen.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// The keystroke that does the same thing from anywhere, for a reader and for the eye.
    #[prop(into, optional)]
    shortcut: Option<String>,
    /// Classes merged after the item's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the item reads as.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, MenubarStyle::CSS);
    let node = NodeRef::new();
    let item = use_roving_item(node);
    let menu = MenubarMenuContext::current();
    let on_leave = defocus_on_leave(node, menu.as_ref().map(MenubarMenuContext::content));
    let told = StoredValue::new_local(on_select);

    let mut semantics = A11yBinding::new(Role::MenuItem).disabled(move || disabled.get());
    if let Some(keys) = shortcut.clone() {
        semantics = semantics.keyboard_shortcut(keys);
    }

    let own = Attrs::new()
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from(semantics);

    let hint = shortcut.map(|keys| view! { text(class = "zui-menubar__shortcut") {{keys}} });

    view! {
        control(
            class = "zui-menubar__item",
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
                if let Some(told) = told.get_value() {
                    told.run(());
                }
                if let Some(menu) = &menu {
                    menu.dismiss();
                }
            },
            {..own},
            {..attrs},
            class = class
        ) {
            box(class = "zui-menubar__item-label") {{children.into_view_once()}}
            {hint}
        }
    }
}

/// A rule between two runs of menu items.
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
///         MenubarContent {
///             MenubarItem {"New"}
///             MenubarSeparator()
///             MenubarItem {"Quit"}
///         }
///     }}
/// }
/// # }
/// ```
///
/// Not an item: it registers with nothing, so the arrow keys step straight over it.
#[component]
pub fn MenubarSeparator(
    /// Classes merged after the rule's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, MenubarStyle::CSS);
    let own = Attrs::new().a11y_from(A11yBinding::unspecified().hidden(true));

    view! { box(class = "zui-menubar__separator", {..own}, {..attrs}, class = class) }
}

/// A heading over a run of menu items.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Menubar {MenubarMenu(value = "view") {
///         MenubarTrigger {"View"}
///         MenubarContent {
///             MenubarLabel {"Appearance"}
///             MenubarItem {"Zoom in"}
///         }
///     }}
/// }
/// # }
/// ```
#[component]
pub fn MenubarLabel(
    /// Classes merged after the heading's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the heading says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, MenubarStyle::CSS);

    view! {
        label(class = "zui-menubar__label", {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// The keystroke that does the same thing as an item, written at its right-hand end.
///
/// A picture of the keystroke, kept out of the accessibility tree: a reader is told about a
/// shortcut by the item itself, because [`MenubarItem`]'s `shortcut` prop draws one of these *and*
/// declares it. Writing this by hand is for a layout that needs the mark somewhere else.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui::menubar::{MenubarShortcut, MenubarShortcutProps};
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Menubar {MenubarMenu(value = "file") {
///         MenubarTrigger {"File"}
///         MenubarContent {
///             MenubarItem {"Save as…" MenubarShortcut {"S"}}
///         }
///     }}
/// }
/// # }
/// ```
#[component]
pub fn MenubarShortcut(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The keystroke.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, MenubarStyle::CSS);
    let own = Attrs::new().a11y_from(A11yBinding::unspecified().hidden(true));
    view! {
        text(class = "zui-menubar__shortcut", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
