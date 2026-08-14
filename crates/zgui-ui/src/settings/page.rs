//! One entry in the page list.

use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::vocab::UiState;
use zgui::{component, view};
use zgui_ui_primitives::use_roving_item_when;

use crate::settings::context::SettingsContext;
use crate::settings::style;

/// One entry of a [`SettingsPages`](crate::SettingsPages).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Settings(default_page = "appearance") {
///         SettingsPages {
///             SettingsPage(value = "appearance") {"Appearance"}
///             SettingsPage(value = "experiments", disabled = true) {"Experiments"}
///         }
///         SettingsPane(value = "appearance") {text {"Colours and type."}}
///     }
/// }
/// # }
/// ```
///
/// The children are whatever names the page: a word, or a mark and a word. An entry is a control
/// and its children are read as its name, so anything decorative inside it belongs to the mark
/// rather than to the entry.
///
/// # Arrowing to a page shows it
///
/// The focus and the shown pane move together, so there is never an entry highlighted beside a
/// pane that belongs to a different one. A pane of preferences costs a layout and nothing else,
/// which is what makes that the right way round here.
#[component]
pub fn SettingsPage(
    /// Which pane this entry shows.
    #[prop(into)]
    value: String,
    /// Whether this entry can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What names the page.
    children: Children,
) -> impl IntoView {
    style::install();
    let context = SettingsContext::current();
    let name = Rc::new(value);
    let node = context.map_or_else(NodeRef::new, |settings| settings.entry_of(&name));
    // Skipped by the arrow keys while it is disabled rather than merely refusing to be chosen: the
    // list shows whatever the arrows land on, so an arrow that stopped here would leave the mark on
    // this entry and the pane of the one before it, which is one interface saying two things.
    let item = use_roving_item_when(node, Signal::derive_local(move || !disabled.get()));

    let selected = {
        let name = Rc::clone(&name);
        move || context.is_some_and(|settings| settings.is_selected(&name))
    };
    let show = {
        let name = Rc::clone(&name);
        move || {
            if disabled.get_untracked() {
                return;
            }
            if let Some(settings) = context {
                settings.select(&name);
            }
        }
    };

    let mut semantics = A11yBinding::new(Role::Tab)
        .selected(selected.clone())
        .disabled(move || disabled.get());
    if let Some(settings) = context {
        semantics = semantics.controls(settings.pane_of(&name));
    }

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-state"), {
            let selected = selected.clone();
            move || Some(if selected() { "active" } else { "inactive" }.to_owned())
        })
        .attribute(zgui::view::AttrName::new("data-value"), {
            let name = Rc::clone(&name);
            move || Some(name.to_string())
        })
        .attribute(zgui::view::AttrName::new("data-disabled"), move || {
            Some(if disabled.get() { "true" } else { "false" }.to_owned())
        })
        .state(UiState::CHECKED, selected)
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from(semantics);

    let on_focus = {
        let show = show.clone();
        move |_: &mut EventCx<'_, events::FocusIn>| {
            if let Some(item) = item {
                item.activate();
            }
            show();
        }
    };

    view! {
        control(
            class = "zui-settings__page",
            node_ref = node,
            tabindex = move || item.map_or(Focus::Sequential, |item| item.tabindex().get()),
            on:focus_in = on_focus,
            on:click = move |_| show(),
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}
