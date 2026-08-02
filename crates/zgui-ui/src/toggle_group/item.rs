//! One toggle inside a [`ToggleGroup`](crate::ToggleGroup).

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::vocab::UiState;
use zgui::{component, view};
use zgui_ui_primitives::use_roving_item;

use crate::support::variant_attrs;
use crate::toggle::{
    SHEET as TOGGLE_SHEET, ToggleSize, ToggleStyle, ToggleVariant, ToggleVariants,
};
use crate::toggle_group::{ToggleGroupContext, ToggleSelection};

/// One toggle in a group.
///
/// It reads whether it is on from the group rather than holding an answer of its own, which is
/// what makes a single-selection group work at all: pressing one has to turn another off, and a
/// component that owned its own state could only find that out afterwards.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Two marks that can both be on.
/// #[component]
/// fn Marks() -> impl IntoView {
///     view! {
///         ToggleGroup(selection = ToggleSelection::Multiple, label = "Formatting") {
///             ToggleGroupItem(value = "bold", label = "Bold") {"B"}
///             ToggleGroupItem(value = "italic", label = "Italic") {"I"}
///         }
///     }
/// }
/// ```
#[component]
pub fn ToggleGroupItem(
    /// What this item stands for, which is what the group reports.
    #[prop(into)]
    value: String,
    /// How it looks, when it differs from the rest of the group.
    #[prop(into, optional)]
    variant: Option<ToggleVariant>,
    /// How big it is, when it differs from the rest of the group.
    #[prop(into, optional)]
    size: Option<ToggleSize>,
    /// Whether this one item can be operated, whatever the group says.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// What it is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Where to record this item's own element, for relating it to something else.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the item's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it shows.
    children: Children,
) -> impl IntoView {
    install_stylesheet(TOGGLE_SHEET, ToggleStyle::CSS);
    let node = node_ref.unwrap_or_default();
    let item = use_roving_item(node);
    let group = ToggleGroupContext::current();
    // The group's look, unless this item asked for one. A strip is described once, and an item
    // that wants to differ says so — rather than every item repeating what the strip already said.
    let variants = ToggleVariants {
        variant: variant
            .or_else(|| group.map(|group| group.variant()))
            .unwrap_or(ToggleVariant::Default),
        size: size
            .or_else(|| group.map(|group| group.size()))
            .unwrap_or(ToggleSize::Md),
    };

    let on = {
        let value = value.clone();
        move || group.is_some_and(|group| group.is_on(&value))
    };
    let is_out = move || disabled.get() || group.is_some_and(|group| group.is_disabled());

    // A single-selection group is a set of alternatives, and a reader is told so: its items are
    // radio buttons, because "one of these" is what a radio button means. A multiple-selection
    // group's items stay buttons that report whether they are pressed.
    let role = match group.map(|group| group.selection()) {
        Some(ToggleSelection::Single) => Role::RadioButton,
        _ => Role::Button,
    };
    let mut semantics = A11yBinding::new(role)
        .toggled_on(on.clone())
        .disabled(is_out);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }

    let own = variant_attrs(variants.classes(), variants.data_attributes())
        .class_toggle(zgui::view::ClassName::new("zui-toggle-group__item"), true)
        .attribute(zgui::view::AttrName::new("data-state"), {
            let on = on.clone();
            move || Some(if on() { "on" } else { "off" }.to_owned())
        })
        .attribute(zgui::view::AttrName::new("data-value"), {
            let value = value.clone();
            move || Some(value.clone())
        })
        .state(UiState::CHECKED, on)
        .state(UiState::DISABLED, is_out)
        .a11y_from(semantics);

    view! {
        control(
            class = ToggleStyle::CLASS,
            node_ref = node,
            tabindex = move || item.map_or(Focus::Sequential, |item| item.tabindex().get()),
            on:focus_in = move |_| { if let Some(item) = item { item.activate() } },
            on:click = move |_| {
                if !is_out() && let Some(group) = group {
                    group.toggle(&value);
                }
            },
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}
