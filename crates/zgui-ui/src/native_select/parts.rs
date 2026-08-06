//! What a [`NativeSelect`](crate::NativeSelect) is filled with.

use zgui::prelude::*;
use zgui::view::AttrName;
use zgui::vocab::UiState;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::mark::CHECK;

use crate::listbox::{ListboxOption, use_listbox_option};
use crate::native_select::style::NativeSelectStyle;
use crate::native_select::{FirstShown, SHEET};
use crate::support::activate_on_press;

/// One thing that can be chosen.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { NativeSelect {NativeSelectOption(value = "a4") {"A4"}} }
/// # }
/// ```
#[component]
pub fn NativeSelectOption(
    /// What choosing it reports.
    #[prop(into)]
    value: String,
    /// What it reads as, when that is not what it shows.
    ///
    /// Left out, it is the text the option actually renders — so an option written as a plain
    /// string says its own label once, and the closed control shows exactly what the list did.
    #[prop(into, optional)]
    text: Option<String>,
    /// Whether it can be chosen.
    #[prop(default = false)]
    disabled: bool,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, NativeSelectStyle::CSS);
    let node = NodeRef::new();
    let out = Signal::stored_local(disabled);
    let reads_as = text.unwrap_or_default();
    let option = ListboxOption::new(node, value.clone(), reads_as, out);
    // Offered whether this build is the open list or the hidden catalogue: the slot takes only the
    // first offer, so the control shows the first option before anything has been chosen — which
    // is what the platform's own select would show.
    if let Some(first) = FirstShown::current() {
        first.offer(option.clone());
    }
    let registered = use_listbox_option(option);

    let chosen = {
        let value = value.clone();
        move || registered.is_some_and(|(listbox, _)| listbox.is_chosen(&value))
    };
    let active = move || registered.is_some_and(|(listbox, id)| listbox.active() == Some(id));

    let own = Attrs::new()
        .attribute(AttrName::new("data-value"), {
            let value = value.clone();
            move || Some(value.clone())
        })
        .attribute(AttrName::new("data-state"), {
            let chosen = chosen.clone();
            move || Some(if chosen() { "checked" } else { "unchecked" }.to_owned())
        })
        .attribute(AttrName::new("data-active"), move || {
            active().then(|| "true".to_owned())
        })
        .attribute(AttrName::new("data-disabled"), move || {
            disabled.then(|| "true".to_owned())
        })
        .state(UiState::CHECKED, chosen.clone())
        .state(UiState::DISABLED, move || disabled)
        .a11y_from(
            A11yBinding::new(Role::ListBoxOption)
                .selected(chosen.clone())
                .disabled(disabled),
        );

    let take = move || {
        if let Some((listbox, id)) = registered {
            listbox.take(id);
        }
    };

    view! {
        box(
            node_ref = node,
            class = "zui-native-select__option",
            on:pointer_down = activate_on_press(),
            on:click = move |_| take(),
            on:pointer_enter = move |_| {
                if !disabled && let Some((listbox, id)) = registered {
                    listbox.set_active(Some(id));
                }
            },
            {..own},
            {..attrs},
            class = class
        ) {
            box(class = "zui-native-select__indicator") {
                Show(when = chosen.clone(), fallback = || ()) {
                    Icon(icon = CHECK)
                }
            }
            {children.into_view_once()}
        }
    }
}

/// A named run of [`NativeSelectOption`]s.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     NativeSelect {
///         NativeSelectOptGroup(label = "Metric") {NativeSelectOption(value = "a4") {"A4"}}
///     }
/// }
/// # }
/// ```
#[component]
pub fn NativeSelectOptGroup(
    /// What the run is called.
    #[prop(into)]
    label: String,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The options in it.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, NativeSelectStyle::CSS);
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::Group).label(label.clone()));
    // The heading is drawn for the eye and hidden from a reader, which already has the group's
    // name from the label relation above — a heading read as well would say every name twice.
    let heading = Attrs::new().a11y_from(A11yBinding::unspecified().hidden(true));

    view! {
        box(class = "zui-native-select__optgroup", {..own}, {..attrs}, class = class) {
            text(class = "zui-native-select__optgroup-label", {..heading}) {{label}}
            {children.into_view_once()}
        }
    }
}
