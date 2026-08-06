//! Menu items that carry a setting rather than a command.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::view::{AttrName, ClassName};
use zgui::vocab::UiState;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::mark::{CHECK, DOT};
use zgui_ui_primitives::{Binding, Controllable, use_roving_item_when};

use crate::menu::content::MenuSurface;
use crate::menu::item::defocus_on_leave;
use crate::menu::style::MenuStyle;
use crate::menu::{MenuContext, SHEET};
use crate::support::activate_on_press;

/// A menu item that is on or off.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Which columns a table shows.
/// #[component]
/// fn Columns() -> impl IntoView {
///     let totals = RwSignal::new_local(true);
///     view! {
///         DropdownMenu {
///             DropdownMenuTrigger {"Columns"}
///             DropdownMenuContent {
///                 MenuCheckboxItem(checked = totals, close_on_select = false) {"Totals"}
///             }
///         }
///     }
/// }
/// ```
///
/// # Who owns it
///
/// Whoever wants to, as everywhere else here: pass `checked` and the caller owns it, leave it out
/// and the item does. `on_change` is told either way.
#[component]
pub fn MenuCheckboxItem(
    /// Whether it is on, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    checked: Binding<bool>,
    /// Whether it starts on, when the item owns that itself.
    #[prop(default = false)]
    default_checked: bool,
    /// Told whenever it changes, whoever owns it.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<bool>>,
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
    install_stylesheet(SHEET, MenuStyle::CSS);
    let node = NodeRef::new();
    let item = use_roving_item_when(node, Signal::derive_local(move || !disabled.get()));
    let menu = MenuContext::current();
    let value = Controllable::new(checked, default_checked, on_change);
    let on_leave = defocus_on_leave(node, MenuSurface::current().map(MenuSurface::node));

    let select = move || {
        if disabled.get_untracked() {
            return;
        }
        value.toggle();
        if close_on_select && let Some(menu) = menu {
            menu.dismiss();
        }
    };

    let own = Attrs::new()
        .class_toggle(ClassName::new("zui-menu__item"), true)
        .class_toggle(ClassName::new("zui-menu__item--check"), true)
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
            on:pointer_down = activate_on_press(),
            on:click = move |_| select(),
            on:pointer_enter = move |_| {
                if !disabled.get_untracked() {
                    if let Some(item) = item {
                        item.activate();
                    }
                    node.focus();
                }
            },
            // And leaving takes it back, so nothing stays lit where the pointer used to be.
            on:pointer_leave = on_leave,
            on:focus_in = move |_| { if let Some(item) = item { item.activate() } },
            {..own},
            {..attrs},
            class = class
        ) {
            // The tick keeps its column whether it is showing or not, so a run of items lines up.
            box(class = "zui-menu__indicator") {
                if move || value.get() {
                    Icon(icon = CHECK)
                } else {}
            }
            {children.into_view_once()}
        }
    }
}

/// What a [`MenuRadioItem`] reads to know whether it is the chosen one.
#[derive(Copy, Clone)]
pub struct MenuRadioContext {
    /// Which value is chosen, owned by whoever asked to own it.
    ///
    /// Nothing chosen is the empty string, which is not a value any item can have: an item is
    /// named by what choosing it reports, and a choice that reports nothing is not one.
    held: Controllable<String>,
}

impl MenuRadioContext {
    /// The group an item is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Which value is chosen right now.
    #[must_use]
    pub fn chosen(&self) -> Option<String> {
        Some(self.held.get()).filter(|chosen| !chosen.is_empty())
    }

    /// Whether `value` is the chosen one.
    #[must_use]
    pub fn is_chosen(&self, value: &str) -> bool {
        self.chosen().is_some_and(|chosen| chosen == value)
    }

    /// Chooses `value`, and tells whoever asked to be told.
    pub fn choose(&self, value: &str) {
        self.held.set(value.to_owned());
    }
}

/// A run of menu items out of which exactly one is chosen.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// How a list is sorted.
/// #[component]
/// fn SortBy() -> impl IntoView {
///     let order = RwSignal::new_local("date".to_owned());
///     view! {
///         DropdownMenu {
///             DropdownMenuTrigger {"Sort"}
///             DropdownMenuContent {
///                 MenuRadioGroup(value = order) {
///                     MenuRadioItem(value = "date") {"Date"}
///                     MenuRadioItem(value = "amount") {"Amount"}
///                 }
///             }
///         }
///     }
/// }
/// ```
#[component]
pub fn MenuRadioGroup(
    /// Which value is chosen, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    value: Binding<String>,
    /// Which value starts chosen, when the group owns it itself.
    #[prop(into, optional)]
    default_value: Option<String>,
    /// Told whenever the choice changes.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<String>>,
    /// What the group is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The choices.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, MenuStyle::CSS);
    provide_local_context(MenuRadioContext {
        held: Controllable::new(value, default_value.unwrap_or_default(), on_change),
    });

    let mut semantics = A11yBinding::new(Role::Group);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    let own = Attrs::new()
        .class_toggle(ClassName::new("zui-menu__group"), true)
        .a11y_from(semantics);

    view! { box({..own}, {..attrs}, class = class) {{children.into_view_once()}} }
}

/// One choice inside a [`MenuRadioGroup`].
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     DropdownMenu {DropdownMenuContent {
///         MenuRadioGroup(default_value = "date") {
///             MenuRadioItem(value = "date") {"Date"}
///         }
///     }}
/// }
/// # }
/// ```
#[component]
pub fn MenuRadioItem(
    /// What choosing this one means, which is what the group reports.
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
    install_stylesheet(SHEET, MenuStyle::CSS);
    let node = NodeRef::new();
    let item = use_roving_item_when(node, Signal::derive_local(move || !disabled.get()));
    let menu = MenuContext::current();
    let group = MenuRadioContext::current();
    let on_leave = defocus_on_leave(node, MenuSurface::current().map(MenuSurface::node));

    let chosen = {
        let value = value.clone();
        move || group.is_some_and(|group| group.is_chosen(&value))
    };
    let select = {
        let value = value.clone();
        move || {
            if disabled.get_untracked() {
                return;
            }
            if let Some(group) = group {
                group.choose(&value);
            }
            if close_on_select && let Some(menu) = menu {
                menu.dismiss();
            }
        }
    };

    let own = Attrs::new()
        .class_toggle(ClassName::new("zui-menu__item"), true)
        .class_toggle(ClassName::new("zui-menu__item--check"), true)
        .attribute(AttrName::new("data-state"), {
            let chosen = chosen.clone();
            move || Some(if chosen() { "checked" } else { "unchecked" }.to_owned())
        })
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
            on:pointer_down = activate_on_press(),
            on:click = move |_| select(),
            on:pointer_enter = move |_| {
                if !disabled.get_untracked() {
                    if let Some(item) = item {
                        item.activate();
                    }
                    node.focus();
                }
            },
            // And leaving takes it back, so nothing stays lit where the pointer used to be.
            on:pointer_leave = on_leave,
            on:focus_in = move |_| { if let Some(item) = item { item.activate() } },
            {..own},
            {..attrs},
            class = class
        ) {
            box(class = "zui-menu__indicator", class = "zui-menu__indicator--dot") {
                Show(when = chosen.clone(), fallback = || ()) {
                    Icon(icon = DOT)
                }
            }
            {children.into_view_once()}
        }
    }
}
