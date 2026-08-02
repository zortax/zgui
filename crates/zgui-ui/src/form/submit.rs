//! The button that sends a form.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};

use crate::button::{ButtonProps, ButtonSize, ButtonVariant};
use crate::form::FormContext;

/// The button that sends the [`Form`](crate::Form) it is written inside.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::{RwSignal, UnsyncCallback};
/// use zgui::{component, view};
/// use zgui_ui::form::Validator;
/// use zgui_ui::prelude::*;
///
/// /// An address, a rule it has to pass, and a way to send it.
/// #[component]
/// fn SignUp() -> impl IntoView {
///     let email = RwSignal::new_local(String::new());
///     let rule = Validator::new(move || {
///         (!email.get().contains('@')).then(|| "An address is needed.".to_owned())
///     });
///     view! {
///         Form(on_submit = UnsyncCallback::new(move |()| { let _ = email.get_untracked(); })) {
///             FormField(name = "email", validate = rule) {
///                 FormItem {
///                     FormLabel {"Email"}
///                     FormMessage()
///                 }
///             }
///             FormSubmit {"Sign up"}
///         }
///     }
/// }
/// ```
///
/// # What pressing it does
///
/// Turns every field's message on, whatever happens — so a press that sends nothing says *why*
/// rather than appearing to do nothing at all — and then either runs the form's `on_submit` or puts
/// the keyboard on the first field that is wrong.
///
/// # Why this rather than a [`Button`](crate::Button)
///
/// The form it belongs to is a context, and the button has to find it *where it is written* rather
/// than where it is pressed. Written by hand that is one line to get wrong and no way to tell: a
/// button that looked its form up somewhere the context does not reach compiles, draws, presses,
/// and does nothing.
#[component]
pub fn FormSubmit(
    /// How it looks.
    #[prop(default = ButtonVariant::Default)]
    variant: ButtonVariant,
    /// How big it is.
    #[prop(default = ButtonSize::Md)]
    size: ButtonSize,
    /// Whether it can be pressed.
    ///
    /// Nothing to do with whether the form is valid: a button that disabled itself while something
    /// was wrong would be a button that never explains what, and the messages are what a person
    /// needs. Reserve it for the send that is already under way.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Where to record this component's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the button's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The label.
    children: Children,
) -> impl IntoView {
    // Read here, in the scope the button is written in, which is the only scope the form it
    // belongs to is published to.
    let form = FormContext::current();

    view! {
        Button(
            variant = variant,
            size = size,
            disabled = disabled,
            node_ref = node_ref.unwrap_or_default(),
            on:click = move |_| {
                if disabled.get_untracked() {
                    return;
                }
                if let Some(form) = form {
                    form.submit();
                }
            },
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}
