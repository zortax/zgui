//! Several lines of text the user types.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::{component, view};

use crate::input::{TextareaStyle, field};
use zgui_ui_primitives::Binding;

/// What the area's rules are installed under.
///
/// Its own name, not the single-line field's: installing under a name already in use replaces that
/// sheet's text, so two components sharing one name means whichever mounted last is the only one
/// styled at all.
const SHEET: &str = "zui-textarea";

/// A text field of several lines.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Somewhere to say what went wrong.
/// #[component]
/// fn Report() -> impl IntoView {
///     let body = RwSignal::new_local(String::new());
///     view! { Textarea(value = body, label = "What happened", placeholder = "Steps to reproduce") }
/// }
/// ```
///
/// # Where the text lives
///
/// Exactly where an [`Input`](crate::Input)'s does: in the element, under the framework's editing
/// model. This component holds no text, no caret and no selection of its own.
///
/// # Keyboard
///
/// The same as an [`Input`](crate::Input)'s, with one difference that is the whole reason this is a
/// separate component: <kbd>Enter</kbd> puts a line break in rather than being left for whatever is
/// around the field to act on. That difference is the element's — `editor` rather than `field` — so
/// it is the editing model that makes it, not a key handler here. <kbd>Tab</kbd> still leaves,
/// because a text area that swallowed tab would be one nobody can get out of without a mouse.
#[component]
pub fn Textarea(
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
    install_stylesheet(SHEET, TextareaStyle::CSS);
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
        role: Role::MultilineTextInput,
    }
    .wire();
    let on_input = field::reporting(value, on_change);

    view! {
        editor(
            node_ref = element,
            class = TextareaStyle::CLASS,
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
