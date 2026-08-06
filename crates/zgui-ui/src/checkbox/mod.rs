//! A control with three positions and a tick in it.

mod state;
mod style;

pub use crate::checkbox::state::Checked;
pub use crate::checkbox::style::CheckboxStyle;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::vocab::UiState;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::mark::{CHECK, MINUS};
use zgui_ui_primitives::{Binding, Controllable};

use crate::support::activate_on_press;

/// What the checkbox's rules are installed under.
const SHEET: &str = "zui-checkbox";

/// A checkbox.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A setting that is on or off.
/// #[component]
/// fn Setting() -> impl IntoView {
///     let name = NodeRef::new();
///     let box_ = NodeRef::new();
///     let on = RwSignal::new_local(Checked::No);
///     view! {
///         row {
///             Checkbox(node_ref = box_, checked = on, labelled_by = name)
///             Label(node_ref = name, control = box_) {"Send me email"}
///         }
///     }
/// }
/// ```
///
/// # Who owns the value
///
/// Whoever wants to. Leaving `checked` out makes the checkbox the owner. Binding a writable
/// signal to it makes the caller the owner and the box writes it back, so the example above is a
/// working checkbox with no callback in it. A value the caller computes rather than stores is
/// bound with [`Binding::controlled`], which takes the write side as an argument — so the box
/// reports what a press would change it to and does not move until the caller moves it.
/// `on_change` is told in all three, after the binding has been asked.
///
/// # Keyboard
///
/// <kbd>Space</kbd> toggles it, and — as with every control here — that is the framework
/// activating what has focus rather than a key handler of this component's. <kbd>Enter</kbd> does
/// the same, which is a deliberate difference from a form in a browser, where it would submit.
///
/// # What a reader is told
///
/// A checkbox with all three positions in it: `Checked::Mixed` is announced as mixed rather than
/// as unticked, so a parent box standing for a partly-ticked list says what it means.
#[component]
pub fn Checkbox(
    /// What it is, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    checked: Binding<Checked>,
    /// What it starts as, when it owns it itself.
    #[prop(into, default = Checked::No)]
    default_checked: Checked,
    /// Told whenever it changes, whoever owns it.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<Checked>>,
    /// Whether it can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Whether leaving it as it is would be wrong, which reddens its border and its ring.
    #[prop(into, default = Signal::stored_local(false))]
    invalid: Signal<bool, LocalStorage>,
    /// The element whose text names this one.
    #[prop(optional)]
    labelled_by: Option<NodeRef>,
    /// Where to record this component's own element, for relating it to something else.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the checkbox's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, CheckboxStyle::CSS);
    let element = node_ref.unwrap_or_default();
    let value = Controllable::new(checked, default_checked, on_change);

    let mut semantics = A11yBinding::new(Role::CheckBox)
        .toggled(move || value.get().toggled_state())
        .disabled(move || disabled.get())
        // Only when it is: a tree says nothing about a control whose answer is fine, and reporting
        // `valid` on every box is noise a reader has to listen past.
        .step(move |a11y| {
            if invalid.get() {
                a11y.invalid(zgui::vocab::Invalid::True)
            } else {
                a11y
            }
        });
    if let Some(label) = labelled_by {
        semantics = semantics.labelled_by(label);
    }

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-checkbox"), true)
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(value.get().name().to_owned())
        })
        .state(UiState::CHECKED, move || value.get().is_checked())
        .state(UiState::INDETERMINATE, move || value.get().is_mixed())
        .state(UiState::DISABLED, move || disabled.get())
        .state(UiState::INVALID, move || invalid.get())
        .a11y_from(semantics);

    view! {
        control(
            node_ref = element,
            class = CheckboxStyle::CLASS,
            tabindex = {Focus::Sequential},
            on:pointer_down = activate_on_press(),
            on:click = move |_| {
                if !disabled.get_untracked() {
                    value.set(value.get_untracked().toggled());
                }
            },
            {..own},
            {..attrs},
            class = class
        ) {
            Icon(icon = CHECK, class = "zui-checkbox__tick")
            Icon(icon = MINUS, class = "zui-checkbox__dash")
        }
    }
}
