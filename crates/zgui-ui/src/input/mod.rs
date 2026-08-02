//! One line of text the user types.

pub(crate) mod field;
mod style;

pub use crate::input::style::{InputStyle, TextareaStyle};

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::{component, view};
use zgui_ui_primitives::Binding;

/// What the field's rules are installed under.
const SHEET: &str = "zui-input";

/// A single-line text field.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Somewhere to type an address.
/// #[component]
/// fn Email() -> impl IntoView {
///     let name = NodeRef::new();
///     let field = NodeRef::new();
///     let address = RwSignal::new_local(String::new());
///     view! {
///         column {
///             Label(node_ref = name, control = field) {"Email"}
///             Input(node_ref = field, value = address, labelled_by = name, placeholder = "you@example.com")
///         }
///     }
/// }
/// ```
///
/// # Where the text lives
///
/// In the field's own element, and in the framework's editing model over it — never in this
/// component. Typing, selecting, undo and an input method's provisional text all belong to that
/// model, the caret is drawn from it, and every change it makes is announced back as an input
/// event. So this is a declaration of an editable element and of what it means, and nothing else.
///
/// # Keyboard
///
/// Whatever the framework's editing model does: characters insert, <kbd>Backspace</kbd> and
/// <kbd>Delete</kbd> take out either side of the caret, the arrows and <kbd>Home</kbd> and
/// <kbd>End</kbd> move it, <kbd>Shift</kbd> extends a selection, and undo and redo work.
/// <kbd>Tab</kbd> still leaves the field, and <kbd>Enter</kbd> is left for whatever a form does
/// with it.
///
/// # Who owns the value
///
/// Whoever wants to, as everywhere else here. Leave `value` out and the field owns the text; bind
/// a writable signal and every keystroke is written into it, so the signal says what the field
/// says without a callback copying it across. `on_change` is told either way, on every change, as
/// it happens.
#[component]
pub fn Input(
    /// The text, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    value: Binding<String>,
    /// What it starts as, when the field owns it itself.
    #[prop(into, optional)]
    default_value: Option<String>,
    /// Told on every change, whoever owns it.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<String>>,
    /// What to show while it is empty.
    #[prop(into, optional)]
    placeholder: Option<String>,
    /// Whether it can be typed into.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Whether its value can be changed, while it stays focusable and readable.
    #[prop(into, default = Signal::stored_local(false))]
    read_only: Signal<bool, LocalStorage>,
    /// Whether it must have a value.
    #[prop(into, default = Signal::stored_local(false))]
    required: Signal<bool, LocalStorage>,
    /// Whether what it holds is wrong.
    #[prop(into, default = Signal::stored_local(false))]
    invalid: Signal<bool, LocalStorage>,
    /// What it is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// The element whose text names this one.
    #[prop(optional)]
    labelled_by: Option<NodeRef>,
    /// Where to record this component's own element, for relating it to something else.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the field's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, InputStyle::CSS);
    let element = node_ref.unwrap_or_default();
    let field::Wired { own, initial } = field::Setup {
        element,
        value,
        default_value,
        placeholder,
        disabled,
        read_only,
        required,
        invalid,
        label,
        labelled_by,
        role: Role::TextInput,
    }
    .wire();
    let on_input = field::reporting(value, on_change);

    view! {
        field(
            node_ref = element,
            class = InputStyle::CLASS,
            tabindex = {Focus::Sequential},
            on:input = move |ev| on_input(ev),
            {..own},
            {..attrs},
            class = class
        ) {
            {initial}
        }
    }
}
