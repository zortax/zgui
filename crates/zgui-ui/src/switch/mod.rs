//! A control that is on or off.

mod style;

pub use crate::switch::style::SwitchStyle;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::vocab::UiState;
use zgui::{component, variants, view};
use zgui_ui_primitives::{Binding, Controllable};

use crate::support::{activate_on_press, variant_attrs};

/// What the switch's rules are installed under.
const SHEET: &str = "zui-switch";

variants! {
    /// The axes a [`Switch`] varies along.
    pub SwitchVariants {
        base: "zui-switch",
        size: { Sm => "zui-switch--sm", Md => "" } = Md,
    }
}

/// A switch: on, or off, and it takes effect the moment it is flipped.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A preference that applies as soon as it is changed.
/// #[component]
/// fn DarkMode() -> impl IntoView {
///     let name = NodeRef::new();
///     let control = NodeRef::new();
///     let dark = RwSignal::new_local(false);
///     view! {
///         row {
///             Label(node_ref = name, control = control) {"Dark"}
///             Switch(node_ref = control, checked = dark, labelled_by = name)
///         }
///     }
/// }
/// ```
///
/// # A switch and a checkbox
///
/// They are not the same control and the difference is not decoration. A checkbox states a fact
/// that something else will act on later; a switch *is* the action, and takes effect at once. That
/// is why this has two positions and no mixed one, and why a reader is told it is a switch.
///
/// # Keyboard
///
/// <kbd>Space</kbd> and <kbd>Enter</kbd> flip it, through the framework's own activation of
/// whatever has focus.
#[component]
pub fn Switch(
    /// Whether it is on, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    checked: Binding<bool>,
    /// Whether it starts on, when it owns that itself.
    #[prop(default = false)]
    default_checked: bool,
    /// Told whenever it changes, whoever owns it.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<bool>>,
    /// Whether it can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// How big it is. The small one is for a switch inside a dense row or a menu.
    #[prop(default = SwitchSize::Md)]
    size: SwitchSize,
    /// The element whose text names this one.
    #[prop(optional)]
    labelled_by: Option<NodeRef>,
    /// Where to record this component's own element, for relating it to something else.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the switch's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, SwitchStyle::CSS);
    let element = node_ref.unwrap_or_default();
    let value = Controllable::new(checked, default_checked, on_change);

    let mut semantics = A11yBinding::new(Role::Switch)
        .toggled_on(move || value.get())
        .disabled(move || disabled.get());
    if let Some(label) = labelled_by {
        semantics = semantics.labelled_by(label);
    }

    let variants = SwitchVariants { size };
    let own = variant_attrs(variants.classes(), variants.data_attributes())
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(if value.get() { "checked" } else { "unchecked" }.to_owned())
        })
        .state(UiState::CHECKED, move || value.get())
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from(semantics);

    view! {
        control(
            node_ref = element,
            class = SwitchStyle::CLASS,
            tabindex = {Focus::Sequential},
            on:pointer_down = activate_on_press(),
            on:click = move |_| {
                if !disabled.get_untracked() {
                    value.toggle();
                }
            },
            {..own},
            {..attrs},
            class = class
        ) {
            box(class = "zui-switch__thumb")
        }
    }
}
