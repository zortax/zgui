//! The text fields, and a form that checks one of them.

use zgui::prelude::*;
use zgui::reactive::{RwSignal, UnsyncCallback};
use zgui::{component, view};
use zgui_ui::form::{FormFieldContext, Validator, use_form_field};
use zgui_ui::prelude::*;

use crate::shell::{PanelProps, RowProps};

/// Everything a person types into.
#[component]
pub(crate) fn Fields() -> impl IntoView {
    let name = RwSignal::new_local("Ada Lovelace".to_owned());
    let empty = RwSignal::new_local(String::new());
    let notes = RwSignal::new_local("Two lines of\nnotes.".to_owned());
    let code = RwSignal::new_local("42".to_owned());

    let name_label = NodeRef::new();
    let name_field = NodeRef::new();

    view! {
        Panel(title = "Input and textarea", note = "filled, empty with a placeholder, and disabled") {
            Row(label = "labelled") {
                column(class = "field wide") {
                    Label(node_ref = name_label, control = name_field) {"Display name"}
                    Input(
                        node_ref = name_field,
                        value = name,
                        on_change = UnsyncCallback::new(move |text: String| name.set(text)),
                        labelled_by = name_label
                    )
                }
            }
            Row(label = "placeholder") {
                Input(
                    value = empty,
                    on_change = UnsyncCallback::new(move |text: String| empty.set(text)),
                    placeholder = "you@example.com",
                    label = "Email"
                )
            }
            Row(label = "disabled") {
                Input(value = empty, placeholder = "Locked", label = "Locked", disabled = true)
            }
            Row(label = "textarea") {
                Textarea(
                    value = notes,
                    on_change = UnsyncCallback::new(move |text: String| notes.set(text)),
                    label = "Notes",
                    placeholder = "What happened"
                )
            }
        }

        Panel(title = "One-time code", note = "one box per character, in runs with a mark between") {
            Row(label = "six boxes") {
                InputOtp(
                    value = code,
                    on_change = UnsyncCallback::new(move |text: String| code.set(text)),
                    length = 6_usize,
                    label = "Confirmation code"
                )
            }
            Row(label = "three and three") {
                InputOtp(
                    length = 6_usize,
                    groups = vec![3_usize, 3_usize],
                    label = "Grouped code"
                )
            }
            Row(label = "invalid") {
                InputOtp(length = 4_usize, invalid = true, label = "Wrong code")
            }
        }

        Panel(title = "Sizes", note = "the controls that come in a second, smaller size") {
            Row(label = "select") {
                Select() {
                    SelectTrigger(size = SelectTriggerSize::Sm, a11y:label = "Small select") {
                        SelectValue(placeholder = "Small")
                    }
                    SelectContent {SelectItem(value = "a") {"One"}}
                }
                Select() {
                    SelectTrigger(a11y:label = "Ordinary select") {
                        SelectValue(placeholder = "Ordinary")
                    }
                    SelectContent {SelectItem(value = "a") {"One"}}
                }
            }
            Row(label = "switch") {
                Switch(size = SwitchSize::Sm, a11y:label = "Small")
                Switch(a11y:label = "Ordinary")
            }
        }

        Panel(title = "Form", note = "a field that says what is wrong with it") {
            SignUp()
        }
    }
}

/// A form with one field, one rule and something to send it with.
#[component]
fn SignUp() -> impl IntoView {
    let email = RwSignal::new_local("not-an-address".to_owned());
    let check = Validator::new(move || {
        let value = email.get();
        if value.is_empty() {
            Some("An address is needed.".to_owned())
        } else if !value.contains('@') {
            Some("That does not look like an address.".to_owned())
        } else {
            None
        }
    });

    view! {
        Form(on_submit = UnsyncCallback::new(move |()| { let _ = email.get_untracked(); })) {
            FormField(name = "email", validate = check) {
                FormItem {
                    FormLabel {"Email"}
                    EmailInput(value = email)
                    FormDescription {"We only write about this account."}
                    FormMessage()
                }
            }
            FormSubmit {"Sign up"}
        }
    }
}

/// The control itself, wired to the field it is inside.
#[component]
fn EmailInput(
    /// What is typed into it.
    value: RwSignal<String, zgui::reactive::LocalStorage>,
) -> impl IntoView {
    let field = use_form_field();
    let attrs = field.map(FormFieldContext::attrs).unwrap_or_default();
    view! {
        Input(
            value = value,
            on_change = UnsyncCallback::new(move |text: String| value.set(text)),
            node_ref = field.map(FormFieldContext::control).unwrap_or_default(),
            {..attrs}
        )
    }
}
