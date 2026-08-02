//! One option of a select.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::AttrName;
use zgui::vocab::UiState;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::mark::CHECK;

use crate::listbox::{ListboxOption, use_listbox_option};
use crate::select::SHEET;
use crate::select::style::SelectStyle;

/// One option of a [`Select`](crate::Select).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Select {
///         SelectTrigger {SelectValue()}
///         SelectContent {
///             SelectItem(value = "gbp") {"Pound sterling"}
///             SelectItem(value = "usd", disabled = true) {"US dollar"}
///         }
///     }
/// }
/// # }
/// ```
///
/// # Why it takes its own text
///
/// The closed trigger shows what the chosen option reads as, and an arrow key lands on the option
/// after the one being walked. Both are questions about *text*, and the option is the only thing
/// that has it — so it says so once, here, and neither the trigger nor the keyboard has to be told
/// separately.
#[component]
pub fn SelectItem(
    /// What choosing it reports.
    #[prop(into)]
    value: String,
    /// What it reads as, when that is not what it shows.
    ///
    /// Left out, it is the text the option actually renders — so an option written as a plain
    /// string says its own label once, and the closed trigger shows exactly what the list did.
    #[prop(into, optional)]
    text: Option<String>,
    /// Whether it can be chosen.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Classes merged after the option's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it shows.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, SelectStyle::CSS);
    let node = NodeRef::new();
    let reads_as = text.unwrap_or_default();
    let registered =
        use_listbox_option(ListboxOption::new(node, value.clone(), reads_as, disabled));

    let chosen = {
        let value = value.clone();
        move || registered.is_some_and(|(listbox, _)| listbox.is_chosen(&value))
    };
    let active = move || registered.is_some_and(|(listbox, id)| listbox.active() == Some(id));

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-select__item"), true)
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
            disabled.get().then(|| "true".to_owned())
        })
        .state(UiState::CHECKED, chosen.clone())
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from(
            A11yBinding::new(Role::ListBoxOption)
                .selected(chosen.clone())
                .disabled(move || disabled.get()),
        );

    let take = move || {
        if let Some((listbox, id)) = registered {
            listbox.take(id);
        }
    };

    view! {
        box(
            node_ref = node,
            on:click = move |_| take(),
            on:pointer_enter = move |_| {
                if !disabled.get_untracked()
                    && let Some((listbox, id)) = registered
                {
                    listbox.set_active(Some(id));
                }
            },
            {..own},
            {..attrs},
            class = class
        ) {
            box(class = "zui-select__indicator") {
                Show(when = chosen.clone(), fallback = || ()) {
                    Icon(icon = CHECK)
                }
            }
            {children.into_view_once()}
        }
    }
}
