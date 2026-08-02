//! One choice inside a [`RadioGroup`](crate::RadioGroup).

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::vocab::UiState;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::mark::DISC;
use zgui_ui_primitives::use_roving_item;

use crate::radio_group::style::RadioItemStyle;
use crate::radio_group::{ITEM_SHEET, RadioContext};

/// One choice.
///
/// It is only meaningful inside a [`RadioGroup`](crate::RadioGroup), which is where its value,
/// its keyboard behaviour and the question of which one is chosen all come from. Outside one it
/// still renders and is still focusable, and it chooses nothing — which is what a choice with
/// nothing to be chosen out of amounts to.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Two choices with names beside them.
/// #[component]
/// fn Size() -> impl IntoView {
///     let size = RwSignal::new_local("small".to_owned());
///     let small = NodeRef::new();
///     view! {
///         RadioGroup(value = size, label = "Size") {
///             row {
///                 RadioGroupItem(value = "small", labelled_by = small)
///                 Label(node_ref = small) {"Small"}
///             }
///             RadioGroupItem(value = "large", label = "Large")
///         }
///     }
/// }
/// ```
#[component]
pub fn RadioGroupItem(
    /// What choosing this one means, which is what the group reports.
    #[prop(into)]
    value: String,
    /// Whether this one choice can be operated, whatever the group says.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Whether leaving the group as it is would be wrong, which reddens this choice's ring.
    #[prop(into, default = Signal::stored_local(false))]
    invalid: Signal<bool, LocalStorage>,
    /// What this choice is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// The element whose text names this one, when it is named by something on the surface.
    #[prop(optional)]
    labelled_by: Option<NodeRef>,
    /// Where to record this item's own element, for relating it to something else.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the item's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(ITEM_SHEET, RadioItemStyle::CSS);
    let node = node_ref.unwrap_or_default();
    let item = use_roving_item(node);
    let group = RadioContext::current();

    let chosen = {
        let value = value.clone();
        move || group.is_some_and(|group| group.is_chosen(&value))
    };
    let is_out = move || disabled.get() || group.is_some_and(|group| group.is_disabled());

    let mut semantics = A11yBinding::new(Role::RadioButton)
        .toggled_on(chosen.clone())
        .disabled(is_out)
        .step(move |a11y| {
            if invalid.get() {
                a11y.invalid(zgui::vocab::Invalid::True)
            } else {
                a11y
            }
        });
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    if let Some(target) = labelled_by {
        semantics = semantics.labelled_by(target);
    }
    if let Some(group) = group {
        semantics = semantics.step(move |a11y| {
            a11y.radio_group(zgui::vocab::NodeId(
                group.element().get().map_or(0, |id| id.as_u64()),
            ))
        });
    }

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-radio-group__item"), true)
        .class_toggle(zgui::view::ClassName::new(RadioItemStyle::CLASS), true)
        .attribute(zgui::view::AttrName::new("data-state"), {
            let chosen = chosen.clone();
            move || Some(if chosen() { "checked" } else { "unchecked" }.to_owned())
        })
        .attribute(zgui::view::AttrName::new("data-value"), {
            let value = value.clone();
            move || Some(value.clone())
        })
        .state(UiState::CHECKED, chosen.clone())
        .state(UiState::DISABLED, is_out)
        .state(UiState::INVALID, move || invalid.get())
        .a11y_from(semantics);

    // Choosing happens on focus as well as on press. That is the pattern's own rule rather than a
    // shortcut: arrowing through a radio group chooses as it goes, and a group where it did not
    // would need space pressed after every arrow key.
    let choose = {
        let value = value.clone();
        move || {
            if is_out() {
                return;
            }
            if let Some(item) = item {
                item.activate();
            }
            if let Some(group) = group {
                group.choose(&value);
            }
        }
    };
    let on_click = choose.clone();

    view! {
        control(
            node_ref = node,
            tabindex = move || item.map_or(Focus::Sequential, |item| item.tabindex().get()),
            on:click = move |_| on_click(),
            on:focus_in = move |_| choose(),
            {..own},
            {..attrs},
            class = class
        ) {
            Icon(icon = DISC)
        }
    }
}
