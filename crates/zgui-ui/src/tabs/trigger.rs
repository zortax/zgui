//! One tab in the strip.

use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::vocab::UiState;
use zgui::{component, view};
use zgui_ui_primitives::use_roving_item_when;

use crate::tabs::style::TabsStyle;
use crate::tabs::{SHEET, TabsActivation, TabsContext};

/// One tab of a [`TabsList`](crate::TabsList).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Tabs(default_value = "billing") {
///         TabsList {
///             TabsTrigger(value = "billing") {"Billing"}
///             TabsTrigger(value = "usage", disabled = true) {"Usage"}
///         }
///         TabsContent(value = "billing") {text {"Cards and invoices."}}
///     }
/// }
/// # }
/// ```
///
/// # What a reader is told
///
/// That it is a tab, whether it is the selected one, and which panel it controls — the last as a
/// relation to the panel's own element, so a caller cannot get the two out of step by renaming
/// one of them.
#[component]
pub fn TabsTrigger(
    /// Which panel this tab shows.
    #[prop(into)]
    value: String,
    /// Whether this tab can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Classes merged after the tab's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the tab shows.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, TabsStyle::CSS);
    let context = TabsContext::current();
    let name = Rc::new(value);
    let node = context.map_or_else(NodeRef::new, |tabs| tabs.trigger_of(&name));
    // Skipped by the arrow keys while it is disabled rather than merely refusing to be chosen: a
    // strip that shows whatever the arrows land on would otherwise leave the focus ring on this tab
    // and the panel of the one before it, which is one interface saying two things.
    let item = use_roving_item_when(node, Signal::derive_local(move || !disabled.get()));

    let selected = {
        let name = Rc::clone(&name);
        move || context.is_some_and(|tabs| tabs.is_selected(&name))
    };
    let show = {
        let name = Rc::clone(&name);
        move || {
            if disabled.get_untracked() {
                return;
            }
            if let Some(tabs) = context {
                tabs.select(&name);
            }
        }
    };

    let mut semantics = A11yBinding::new(Role::Tab)
        .selected(selected.clone())
        .disabled(move || disabled.get());
    if let Some(tabs) = context {
        semantics = semantics.controls(tabs.panel_of(&name));
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
        .state(UiState::CHECKED, selected)
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from(semantics);

    // Automatic activation is what makes a tab strip readable one arrow key at a time: the focus
    // and the shown panel move together, so there is never a tab highlighted beside a panel that
    // belongs to a different one. Manual activation is the same strip with the selection left to
    // an explicit press, for a panel that costs something to show.
    let on_focus = {
        let show = show.clone();
        move |_: &mut EventCx<'_, events::FocusIn>| {
            if let Some(item) = item {
                item.activate();
            }
            let automatic = context
                .map(TabsContext::activation)
                .is_some_and(|activation| activation == TabsActivation::Automatic);
            if automatic {
                show();
            }
        }
    };

    view! {
        control(
            class = "zui-tabs__trigger",
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
