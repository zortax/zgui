//! Fields, what is wrong with them, and sending them somewhere.

mod field;
mod parts;
mod style;
mod submit;

pub use crate::form::field::{
    FormField, FormFieldContext, FormFieldProps, Validator, use_form_field,
};
pub use crate::form::parts::{
    FormDescription, FormDescriptionProps, FormItem, FormItemProps, FormLabel, FormLabelProps,
    FormMessage, FormMessageProps,
};
pub use crate::form::style::FormStyle;
pub use crate::form::submit::{FormSubmit, FormSubmitProps};

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, UnsyncCallback};
use zgui::vocab::{Key, NamedKey};
use zgui::{component, view};

/// What the form's rules are installed under.
pub(crate) const SHEET: &str = "zui-form";

/// One field, as the form knows it.
#[derive(Copy, Clone)]
struct Registered {
    /// What it is called inside the form.
    id: u64,
    /// What is wrong with it, when anything is.
    error: Signal<Option<String>, LocalStorage>,
    /// The control itself, so a failed submission can put the focus on it.
    control: NodeRef,
}

/// What every field reads to know whether the form has been sent yet.
#[derive(Copy, Clone)]
pub struct FormContext {
    /// The fields, in the order they were written.
    fields: RwSignal<Vec<Registered>, LocalStorage>,
    /// The next name to hand out.
    next: RwSignal<u64, LocalStorage>,
    /// Whether sending it has been attempted.
    submitted: RwSignal<bool, LocalStorage>,
    /// Told when every field is happy.
    on_submit: StoredValue<Option<UnsyncCallback<()>>, LocalStorage>,
}

impl FormContext {
    /// The form the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Whether sending the form has been attempted.
    ///
    /// What turns the messages on. Before the first attempt a field shows an error only once it
    /// has been left, because a form that is scarlet before a word has been typed is a form that
    /// has told the user off for arriving.
    #[must_use]
    pub fn was_submitted(self) -> bool {
        self.submitted.get()
    }

    /// Whether every field is happy.
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.fields
            .get()
            .iter()
            .all(|field| field.error.get().is_none())
    }

    /// Adds a field, and takes it out again when its scope goes away.
    #[must_use]
    pub fn register(self, error: Signal<Option<String>, LocalStorage>, control: NodeRef) -> u64 {
        let id = self.next.get_untracked();
        self.next.set(id + 1);
        self.fields
            .update(|fields| fields.push(Registered { id, error, control }));
        on_cleanup_local(move || {
            self.fields
                .try_update(|fields| fields.retain(|field| field.id != id));
        });
        id
    }

    /// Tries to send the form, and reports whether it went.
    ///
    /// Every field's messages come on whatever happens, so the user is told what to fix rather
    /// than watching a button do nothing. When something is wrong the focus goes to the first
    /// field that is wrong, in the order the fields were written — which is the order they are on
    /// the screen, and therefore the order somebody would look for them in.
    pub fn submit(self) -> bool {
        self.submitted.set(true);
        let fields = self.fields.get_untracked();
        let first_bad = fields
            .iter()
            .find(|field| field.error.get_untracked().is_some());
        if let Some(field) = first_bad {
            field.control.focus();
            return false;
        }
        if let Some(on_submit) = self.on_submit.get_value() {
            on_submit.run(());
        }
        true
    }

    /// Puts the form back to never having been sent, which is what clearing it means.
    pub fn reset(self) {
        self.submitted.set(false);
    }
}

/// A group of fields, what is wrong with them, and a way to send them.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::{RwSignal, UnsyncCallback};
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
/// use zgui_ui::form::{Validator, use_form_field};
///
/// /// Somewhere to type an address, and something that checks it.
/// #[component]
/// fn SignUp() -> impl IntoView {
///     let email = RwSignal::new_local(String::new());
///     let check = Validator::new(move || {
///         let value = email.get();
///         if value.is_empty() {
///             Some("An address is needed.".to_owned())
///         } else if !value.contains('@') {
///             Some("That does not look like an address.".to_owned())
///         } else {
///             None
///         }
///     });
///
///     view! {
///         Form(on_submit = UnsyncCallback::new(move |()| { let _ = email.get_untracked(); })) {
///             FormField(name = "email", validate = check) {
///                 FormItem {
///                     FormLabel {"Email"}
///                     EmailInput(value = email)
///                     FormDescription {"We will only write about your account."}
///                     FormMessage()
///                 }
///             }
///             FormSubmit {"Sign up"}
///         }
///     }
/// }
///
/// /// The control itself, wired to the field it is inside.
/// #[component]
/// fn EmailInput(value: RwSignal<String, zgui::reactive::LocalStorage>) -> impl IntoView {
///     let field = use_form_field();
///     let attrs = field.map(FormFieldContext::attrs).unwrap_or_default();
///     view! {
///         Input(
///             value = value,
///             node_ref = field.map(FormFieldContext::control).unwrap_or_default(),
///             {..attrs}
///         )
///     }
/// }
/// ```
///
/// # How a control joins in
///
/// Through [`use_form_field`]. A field publishes three things — the element the label points at,
/// the element the message is in, and the attributes that say *invalid* and *described by* — and a
/// control takes them with `node_ref=` and `{..attrs}`. Any control does, including one an
/// application wrote itself, because those are the two things every component in this library
/// takes.
///
/// # Keyboard
///
/// <kbd>Enter</kbd> anywhere in the form that is not a multi-line field sends it, which is what
/// people expect of a form and what a submit button alone does not give.
#[component]
pub fn Form(
    /// Told when the form is sent and every field is happy.
    #[prop(optional)]
    on_submit: Option<UnsyncCallback<()>>,
    /// What the form is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Whether <kbd>Enter</kbd> in a field sends the form.
    #[prop(default = true)]
    submit_on_enter: bool,
    /// Where to record this component's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the form's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The fields.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, FormStyle::CSS);
    let element = node_ref.unwrap_or_default();
    let context = FormContext {
        fields: RwSignal::new_local(Vec::new()),
        next: RwSignal::new_local(1),
        submitted: RwSignal::new_local(false),
        on_submit: StoredValue::new_local(on_submit),
    };
    provide_local_context(context);

    let mut semantics = A11yBinding::new(Role::Form);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-form"), true)
        .attribute(zgui::view::AttrName::new("data-submitted"), move || {
            context.was_submitted().then(|| "true".to_owned())
        })
        .a11y_from(semantics);

    view! {
        column(
            node_ref = element,
            class = FormStyle::CLASS,
            on:key_down = move |ev| {
                // Only a plain Enter, and only when the field it came from did not claim it: a
                // multi-line field stops the key here, and a form that sent itself anyway would be
                // a form nobody can write a paragraph into.
                if submit_on_enter
                    && matches!(ev.key, Key::Named(NamedKey::Enter))
                    && !ev.modifiers.shift()
                {
                    context.submit();
                    ev.prevent_default();
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
