//! The control a select shows its choice on.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::AttrName;
use zgui::vocab::{HasPopup, UiState};
use zgui::{component, variants, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::CHEVRON_DOWN;

use crate::listbox::Listbox;
use crate::overlay::OverlayState;
use crate::select::SHEET;
use crate::select::style::SelectStyle;
use crate::support::variant_attrs;

variants! {
    /// The axes a [`SelectTrigger`] varies along.
    pub SelectTriggerVariants {
        base: "zui-select",
        size: { Sm => "zui-select--sm", Md => "" } = Md,
    }
}

/// The control that shows the choice and opens the list.
///
/// It keeps the keyboard the whole time the list is open — the arrow keys walk the options from
/// here, and the option being walked is pointed at rather than focused.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Select {
///         SelectTrigger {SelectValue(placeholder = "Choose one")}
///         SelectContent {SelectItem(value = "gbp") {"Pound"}}
///     }
/// }
/// # }
/// ```
#[component]
pub fn SelectTrigger(
    /// Whether it can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Whether leaving the choice as it is would be wrong, which reddens its border and its ring.
    #[prop(into, default = Signal::stored_local(false))]
    invalid: Signal<bool, LocalStorage>,
    /// How tall it is. The small one matches a small button in the same row.
    #[prop(default = SelectTriggerSize::Md)]
    size: SelectTriggerSize,
    /// What the control is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// The element whose text names it.
    #[prop(optional)]
    labelled_by: Option<NodeRef>,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it shows, which is usually a [`SelectValue`].
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, SelectStyle::CSS);
    let listbox = Listbox::current();
    let state = listbox.map_or_else(
        || OverlayState::uncontrolled(false, None),
        |listbox| listbox.surface(),
    );

    let on_key_down = handler(
        events::KEY_DOWN,
        move |ev: &mut EventCx<'_, events::KeyDown>| {
            if disabled.get_untracked() {
                return;
            }
            if let Some(listbox) = listbox
                && listbox.handle(&ev.key)
            {
                // Only when it meant something here. Tab, and every key this control does not
                // claim, has to reach whatever is around it.
                ev.prevent_default();
                ev.stop_propagation();
            }
        },
    );

    let mut semantics = A11yBinding::new(Role::ComboBox)
        .has_popup(HasPopup::Listbox)
        .expanded(move || state.is_open())
        .controls(state.content())
        .disabled(move || disabled.get())
        .step(move |a11y| {
            if invalid.get() {
                a11y.invalid(zgui::vocab::Invalid::True)
            } else {
                a11y
            }
        })
        .active_descendant(move || {
            listbox.map_or(zgui::vocab::NodeId(0), |listbox| {
                zgui::vocab::NodeId(
                    listbox
                        .active_node()
                        .get()
                        .map_or(0, zgui::view::NodeId::as_u64),
                )
            })
        });
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    if let Some(target) = labelled_by {
        semantics = semantics.labelled_by(target);
    }

    let variants = SelectTriggerVariants { size };
    let own = variant_attrs(variants.classes(), variants.data_attributes())
        .attribute(AttrName::new("data-state"), move || {
            Some(state.state_name().to_owned())
        })
        .state(UiState::DISABLED, move || disabled.get())
        .state(UiState::INVALID, move || invalid.get())
        .a11y_from(semantics);

    view! {
        control(
            node_ref = {state.trigger()},
            class = {SelectStyle::CLASS},
            tabindex = {Focus::Sequential},
            on:click = move |_| {
                if !disabled.get_untracked() {
                    let was_open = state.is_open_untracked();
                    state.toggle();
                    if !was_open && let Some(listbox) = listbox {
                        listbox.highlight_chosen();
                    }
                }
            },
            on:key_down = on_key_down,
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
            Icon(icon = CHEVRON_DOWN, class = "zui-select__chevron")
        }
    }
}

/// What the chosen option reads as, or a hint when nothing is chosen.
///
/// The text comes from the option itself rather than from a prop, so a select's control and its
/// list cannot say different things about the same value.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Select {SelectTrigger {SelectValue(placeholder = "Currency")}}
/// }
/// # }
/// ```
#[component]
pub fn SelectValue(
    /// What to show while nothing is chosen.
    #[prop(into, optional)]
    placeholder: Option<String>,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, SelectStyle::CSS);
    let listbox = Listbox::current();
    let hint = placeholder.unwrap_or_default();
    let shown = move || {
        listbox
            .and_then(|listbox| listbox.chosen_text())
            .unwrap_or_else(|| hint.clone())
    };
    let empty = move || listbox.is_none_or(|listbox| listbox.chosen_text().is_none());

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-select__placeholder"), empty)
        .state(UiState::PLACEHOLDER_SHOWN, empty);

    view! { text({..own}, {..attrs}, class = class) {{shown}} }
}
