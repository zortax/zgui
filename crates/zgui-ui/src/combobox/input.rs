//! The field a combobox is narrowed with.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::AttrName;
use zgui::vocab::HasPopup;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::CHEVRON_DOWN;

use crate::combobox::SHEET;
use crate::combobox::style::ComboboxStyle;
use crate::input::InputProps;
use crate::input_group::{
    InputGroupAddonAlign, InputGroupAddonProps, InputGroupProps, PARTS_SHEET,
    style::InputGroupPartStyle,
};
use crate::listbox::Listbox;
use crate::overlay::OverlayState;
use zgui_ui_primitives::Binding;

/// The text field of a [`Combobox`](crate::Combobox).
///
/// It is an [`Input`](crate::Input) with the listbox's keyboard model over it and the relations a
/// reader needs: what kind of surface it opens, whether that surface is showing, and which option
/// is being walked.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Combobox {
///         ComboboxInput(placeholder = "Search", label = "Country")
///         ComboboxContent {ComboboxItem(value = "gb") {"United Kingdom"}}
///     }
/// }
/// # }
/// ```
#[component]
pub fn ComboboxInput(
    /// What to show while it is empty.
    #[prop(into, optional)]
    placeholder: Option<String>,
    /// Whether it can be typed into.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// What the field is called, for a reader.
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
) -> impl IntoView {
    install_stylesheet(SHEET, ComboboxStyle::CSS);
    install_stylesheet(PARTS_SHEET, InputGroupPartStyle::CSS);
    let listbox = Listbox::current();
    let state = listbox.map_or_else(
        || OverlayState::uncontrolled(false, None),
        |listbox| listbox.surface(),
    );

    // The list owns what has been typed, so the field reads it from there and writes it back
    // there rather than keeping a second copy that has to be kept in step.
    let typed = Binding::controlled(
        Signal::derive_local(move || listbox.map(|listbox| listbox.filter()).unwrap_or_default()),
        move |text: String| {
            if let Some(listbox) = listbox {
                listbox.set_filter(text);
                // Typing opens it. A field that narrowed a list nobody could see would be a field
                // that does nothing.
                state.open();
            }
        },
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
                ev.prevent_default();
                ev.stop_propagation();
            }
        },
    );

    let mut semantics = A11yBinding::new(Role::ComboBox)
        .has_popup(HasPopup::Listbox)
        .expanded(move || state.is_open())
        .controls(state.content())
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

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-combobox__input"), true)
        .attribute(AttrName::new("data-state"), move || {
            Some(state.state_name().to_owned())
        })
        .a11y_from(semantics);

    // The frame is the group's and the field inside it is bare, so the field, the mark beside it
    // and the ring around the pair read as one control — which is what a combobox is, and what a
    // field with a chevron floating next to it is not.
    view! {
        InputGroup(disabled = disabled) {
            Input(
                node_ref = {state.trigger()},
                value = typed,
                placeholder = placeholder.unwrap_or_default(),
                disabled = disabled,
                class = "zui-input-group__field",
                on:key_down = on_key_down,
                {..own},
                {..attrs},
                class = class
            )
            InputGroupAddon(align = InputGroupAddonAlign::InlineEnd) {
                Icon(icon = CHEVRON_DOWN, class = "zui-combobox__mark")
            }
        }
    }
}
