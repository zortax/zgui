//! A code typed one character per box.

mod parts;
mod style;

pub use crate::input_otp::parts::{
    InputOtpGroup, InputOtpGroupProps, InputOtpSeparator, InputOtpSeparatorProps,
};
pub use crate::input_otp::style::InputOtpStyle;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::vocab::{PropValue, UiState};
use zgui::{component, view};
use zgui_ui_primitives::{Binding, Controllable};

use crate::support::{Edit, apply, key_edit};

/// What the field's rules are installed under.
pub(crate) const SHEET: &str = "zui-otp";

/// How the boxes fall into runs, given how many there are and what the caller asked for.
///
/// A run of nought boxes is dropped and anything beyond `length` is trimmed away, so a caller
/// cannot ask for a gap in front of a box that does not exist. Asking for nothing is one run of the
/// lot, which is what a code with no grouping is.
fn runs(length: usize, asked: &[usize]) -> Vec<usize> {
    let mut left = length;
    let mut runs: Vec<usize> = Vec::new();
    for run in asked {
        let taken = (*run).min(left);
        if taken > 0 {
            runs.push(taken);
            left -= taken;
        }
    }
    if left > 0 {
        runs.push(left);
    }
    runs
}

/// A field for a short code, drawn as one box per character.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// The code from the text message.
/// #[component]
/// fn Confirm() -> impl IntoView {
///     let code = RwSignal::new_local(String::new());
///     view! { InputOtp(value = code, length = 6, label = "Confirmation code") }
/// }
/// ```
///
/// # One control, not six
///
/// The boxes are drawing. There is one focusable element, one value and one place a keystroke
/// goes, which is what makes the whole thing one tab stop and what stops a reader from meeting six
/// unnamed fields. Which box is next is marked with `data-active` for the style sheet, and typing
/// simply appends: the caret is always at the end, because a code is entered forwards.
///
/// # Keyboard
///
/// A character goes into the next empty box, and <kbd>Backspace</kbd> takes the last one out.
/// Anything longer than `length` is refused rather than silently truncating what came before it,
/// and every key the field does not claim — tab, escape, enter — is left for whatever is around
/// it.
#[component]
pub fn InputOtp(
    /// The code so far, when the caller holds it.
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
    /// Told once, when the last box is filled.
    #[prop(optional)]
    on_complete: Option<UnsyncCallback<String>>,
    /// How many characters the code has.
    #[prop(default = 6)]
    length: usize,
    /// How the boxes are broken into runs, with a dash between one run and the next.
    ///
    /// `vec![3, 3]` is the familiar six-figure code. Left out, the boxes are one unbroken run.
    #[prop(optional)]
    groups: Option<Vec<usize>>,
    /// Whether it can be typed into.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Whether the code that has been typed is wrong, which reddens every box.
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
    install_stylesheet(SHEET, InputOtpStyle::CSS);
    let element = node_ref.unwrap_or_default();
    let code = Controllable::new(value, default_value.unwrap_or_default(), on_change);
    let completed = zgui::reactive::StoredValue::new_local(on_complete);

    let mut semantics = A11yBinding::new(Role::TextInput)
        .value(move || zgui::vocab::SharedString::from(code.get()))
        .disabled(move || disabled.get())
        .required(true)
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

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-otp"), true)
        .property(zgui::view::PropKey::new("value"), move || {
            PropValue::from(code.get().as_str())
        })
        .attribute(zgui::view::AttrName::new("data-length"), move || {
            Some(length.to_string())
        })
        .state(UiState::DISABLED, move || disabled.get())
        .state(UiState::INVALID, move || invalid.get())
        .state(UiState::PLACEHOLDER_SHOWN, move || code.get().is_empty())
        .a11y_from(semantics);

    // The runs are settled once, here, and the index each box stands for is carried with it — so a
    // box in the second run reads the second run's characters rather than starting again at nought.
    let mut next = 0_usize;
    let runs: Vec<Vec<usize>> = runs(length, groups.as_deref().unwrap_or(&[]))
        .into_iter()
        .map(|run| {
            let indices = (next..next + run).collect();
            next += run;
            indices
        })
        .collect();
    let last = runs.len().saturating_sub(1);

    view! {
        control(
            node_ref = element,
            class = InputOtpStyle::CLASS,
            tabindex = {Focus::Sequential},
            on:key_down = move |ev| {
                if disabled.get_untracked() {
                    return;
                }
                let Some(edit) = key_edit(&ev.key, false) else { return };
                // Only two edits mean anything here: a code is entered forwards, so there is
                // nowhere for a caret to be but the end.
                let mut text = code.get_untracked();
                let filled = match &edit {
                    Edit::Insert(inserted) => {
                        if text.chars().count() + inserted.chars().count() > length {
                            ev.prevent_default();
                            return;
                        }
                        let end = text.len();
                        apply(&mut text, end, &edit);
                        true
                    }
                    Edit::DeleteBefore => {
                        let end = text.len();
                        apply(&mut text, end, &edit);
                        false
                    }
                    _ => return,
                };
                ev.prevent_default();
                ev.stop_propagation();
                code.set(text.clone());
                if filled
                    && text.chars().count() == length
                    && let Some(on_complete) = completed.get_value()
                {
                    on_complete.run(text);
                }
            },
            {..own},
            {..attrs},
            class = class
        ) {
            {runs
                .into_iter()
                .enumerate()
                .map(|(run, indices)| {
                    let boxes = indices
                        .into_iter()
                        .map(|index| {
                            let character = move || {
                                code.get()
                                    .chars()
                                    .nth(index)
                                    .map_or_else(String::new, String::from)
                            };
                            let active = move || code.get().chars().count() == index;
                            view! {
                                box(
                                    class = "zui-otp__slot",
                                    attr:data-index = move || Some(index.to_string()),
                                    attr:data-active = move || active().then(String::new),
                                    a11y:hidden = true
                                ) {
                                    text {{character}}
                                    box(class = "zui-otp__caret") {
                                        box(class = "zui-otp__bar")
                                    }
                                }
                            }
                        })
                        .collect::<Vec<_>>();
                    let group = view! { InputOtpGroup {{boxes}} };
                    let dash = view! {
                        if move || run < last {
                            InputOtpSeparator()
                        } else {}
                    };
                    (group.into_view(), dash.into_view())
                })
                .collect::<Vec<_>>()}
        }
    }
}
