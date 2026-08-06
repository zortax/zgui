//! One thing a menu offers to do.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::view::AttrName;
use zgui::vocab::UiState;
use zgui::{component, view};
use zgui_ui_primitives::use_roving_item_when;

use crate::menu::MenuContext;
use crate::menu::SHEET;
use crate::menu::content::MenuSurface;
use crate::menu::parts::MenuShortcutProps;
use crate::menu::style::MenuStyle;
use crate::support::activate_on_press;

/// The handler that takes an item's highlight away as the pointer leaves it.
///
/// The highlight is the focus, and focus does not fall off an element on its own: a pointer that
/// slides from an item onto a label, a separator or the menu's padding leaves the item lit as if
/// the pointer were still on it. So the departure mirrors the arrival — the pointer brought the
/// focus here, and it takes it away again, onto `surface`, the panel the item sits on, where it
/// highlights nothing. Only when the item still holds the focus, though: a leave that fires
/// because the keyboard has already moved on must not pull focus off the item the keyboard chose.
pub(crate) fn defocus_on_leave(
    item: NodeRef,
    surface: Option<NodeRef>,
) -> impl Fn(&mut EventCx<'_, events::PointerLeave>) {
    let focused = focused_node();
    move |_| {
        if let Some(surface) = surface
            && focused
                .get_untracked()
                .is_some_and(|holder| item.contains(holder))
        {
            surface.focus();
        }
    }
}

/// One command in a menu.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::UnsyncCallback;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A menu whose items actually do something.
/// #[component]
/// fn Actions() -> impl IntoView {
///     view! {
///         DropdownMenu {
///             DropdownMenuTrigger {"Actions"}
///             DropdownMenuContent {
///                 MenuItem(shortcut = "⌘O", on_select = UnsyncCallback::new(|()| println!("opened"))) {
///                     "Open"
///                 }
///                 MenuItem(disabled = true) {"Restore"}
///                 MenuItem(destructive = true) {"Delete"}
///             }
///         }
///     }
/// }
/// ```
///
/// # Choosing it closes the menu
///
/// All of it, including every submenu it was chosen from — which is what `close_on_select=false`
/// exists to switch off, for the one case where it is wrong: an item that toggles something the
/// user is likely to toggle twice.
///
/// # The highlight is the focus
///
/// Moving the pointer onto an item focuses it, so there is exactly one notion of *where the menu
/// is* and the keyboard and the pointer cannot disagree about it. That is also why nothing here
/// keeps a `highlighted` signal: `:focus` and `:hover` are states the engine already knows.
#[component]
pub fn MenuItem(
    /// Whether it can be chosen.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Whether it is the item that destroys something, which a style sheet colours differently.
    #[prop(default = false)]
    destructive: bool,
    /// Whether it is indented to the column the ticks and bullets leave for a label.
    ///
    /// For a plain command written among a run of choices: without it, its label sits a tick's
    /// width to the left of theirs and the run steps in and out.
    #[prop(default = false)]
    inset: bool,
    /// Whether choosing it closes the menu.
    #[prop(default = true)]
    close_on_select: bool,
    /// Told when it is chosen.
    #[prop(optional)]
    on_select: Option<UnsyncCallback<()>>,
    /// The keystroke that does the same thing, drawn at the item's right-hand end and declared to
    /// a reader as this item's shortcut.
    #[prop(into, optional)]
    shortcut: Option<String>,
    /// Classes merged after the item's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, MenuStyle::CSS);
    let node = NodeRef::new();
    let item = use_roving_item_when(node, Signal::derive_local(move || !disabled.get()));
    let menu = MenuContext::current();
    let on_leave = defocus_on_leave(node, MenuSurface::current().map(MenuSurface::node));

    let select = move || {
        if disabled.get_untracked() {
            return;
        }
        if let Some(on_select) = &on_select {
            on_select.run(());
        }
        if close_on_select && let Some(menu) = menu {
            menu.dismiss();
        }
    };

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-menu__item"), true)
        .class_toggle(
            zgui::view::ClassName::new("zui-menu__item--destructive"),
            destructive,
        )
        .class_toggle(zgui::view::ClassName::new("zui-menu__item--inset"), inset)
        .attribute(AttrName::new("data-destructive"), move || {
            destructive.then(|| "true".to_owned())
        })
        .attribute(AttrName::new("data-inset"), move || {
            inset.then(|| "true".to_owned())
        })
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from({
            let semantics = A11yBinding::new(Role::MenuItem).disabled(move || disabled.get());
            match &shortcut {
                Some(keys) => semantics.keyboard_shortcut(keys.clone()),
                None => semantics,
            }
        });

    view! {
        control(
            node_ref = node,
            tabindex = move || item.map_or(Focus::Sequential, |item| item.tabindex().get()),
            // A menu answers the press, as every desktop's own menus do.
            on:pointer_down = activate_on_press(),
            on:click = move |_| select(),
            // The pointer moves the keyboard rather than shadowing it: one highlight, and the two
            // ways of driving a menu cannot end up pointing at different items.
            on:pointer_enter = move |_| {
                if !disabled.get_untracked() {
                    if let Some(item) = item {
                        item.activate();
                    }
                    node.focus();
                }
            },
            // And leaving takes it back: the pointer moving onto something that is not an item —
            // a label, a separator — is a menu with nothing highlighted, not one still lit where
            // the pointer used to be.
            on:pointer_leave = on_leave,
            on:focus_in = move |_| { if let Some(item) = item { item.activate() } },
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
            {shortcut
                .map(|keys| AnyView::new(view! { MenuShortcut {{keys}} }))
                .unwrap_or_else(|| AnyView::new(()))}
        }
    }
}
