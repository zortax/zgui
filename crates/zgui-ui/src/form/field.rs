//! One field: what is wrong with it, and what a control has to do to join in.

use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::vocab::{Invalid, UiState};
use zgui::{component, view};

use crate::form::style::FormStyle;
use crate::form::{FormContext, SHEET};

/// A rule a field's value has to pass.
///
/// A closure returning the message to show, or `None` when there is nothing wrong. It reads
/// whatever signals it likes, so it is re-checked exactly when one of them changes — there is no
/// "validate now" to forget to call, and no copy of the value for the rule to go stale against.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::{Mounted, RwSignal, install};
/// use zgui_ui::form::Validator;
///
/// install().ok();
/// let scope = Mounted::new();
/// scope.with(|| {
///     let name = RwSignal::new_local(String::new());
///     let required = Validator::new(move || {
///         name.get().is_empty().then(|| "A name is needed.".to_owned())
///     });
///
///     assert_eq!(required.check().as_deref(), Some("A name is needed."));
///     name.set("Ada".to_owned());
///     assert_eq!(required.check(), None);
/// });
/// scope.unmount();
/// ```
#[derive(Clone)]
pub struct Validator(Rc<dyn Fn() -> Option<String>>);

impl Validator {
    /// Wraps a rule.
    #[must_use]
    pub fn new(rule: impl Fn() -> Option<String> + 'static) -> Self {
        Self(Rc::new(rule))
    }

    /// What is wrong right now, when anything is.
    #[must_use]
    pub fn check(&self) -> Option<String> {
        (self.0)()
    }
}

impl core::fmt::Debug for Validator {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("Validator")
    }
}

/// What a field's label, message and control read to find each other.
#[derive(Copy, Clone)]
pub struct FormFieldContext {
    /// What is wrong with the value, whether or not it is being shown.
    error: Signal<Option<String>, LocalStorage>,
    /// Whether the field has been left at least once.
    touched: RwSignal<bool, LocalStorage>,
    /// The form it belongs to, when it is in one.
    form: Option<FormContext>,
    /// The control itself.
    control: NodeRef,
    /// The text that names it.
    label: NodeRef,
    /// The line under the control saying what belongs in it.
    description: NodeRef,
    /// The line saying what is wrong.
    message: NodeRef,
}

impl FormFieldContext {
    /// The field the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// The control itself, for a component to bind with `node_ref`.
    #[must_use]
    pub fn control(self) -> NodeRef {
        self.control
    }

    /// The text that names the control.
    ///
    /// The naming is done from the control's side, because an accessibility tree relates a control
    /// to its name rather than a name to its control — so this is what [`FormFieldContext::attrs`]
    /// puts a `labelled_by` relation on, and a label only has to say where it is.
    #[must_use]
    pub fn label(self) -> NodeRef {
        self.label
    }

    /// The line saying what belongs in the field.
    #[must_use]
    pub fn description(self) -> NodeRef {
        self.description
    }

    /// The line saying what is wrong.
    #[must_use]
    pub fn message(self) -> NodeRef {
        self.message
    }

    /// What is wrong with the value, whether or not it is being shown yet.
    #[must_use]
    pub fn error(self) -> Option<String> {
        self.error.get()
    }

    /// What is wrong *and* worth saying so now.
    ///
    /// Nothing until the field has been left, or until the form has been sent. A form that turns
    /// scarlet before a word has been typed has told the user off for arriving.
    #[must_use]
    pub fn shown_error(self) -> Option<String> {
        let ready = self.touched.get() || self.form.is_some_and(FormContext::was_submitted);
        ready.then(|| self.error.get()).flatten()
    }

    /// Whether the field has been left at least once.
    #[must_use]
    pub fn is_touched(self) -> bool {
        self.touched.get()
    }

    /// Says the field has been left, which is what turns its message on.
    pub fn touch(self) {
        if !self.touched.get_untracked() {
            self.touched.set(true);
        }
    }

    /// What a control has to carry to be part of this field.
    ///
    /// Spread onto any control with `{..field.attrs()}`. It says the control is invalid when it is,
    /// and points its description at whichever of the two lines is worth reading: the hint while
    /// the value is fine, the message once it is not. Two `described_by` relations would be read
    /// out one after the other, and the hint after the complaint is the wrong way round.
    #[must_use]
    pub fn attrs(self) -> Attrs {
        let invalid = move || self.shown_error().is_some();
        Attrs::new()
            .attribute(zgui::view::AttrName::new("data-invalid"), move || {
                invalid().then(|| "true".to_owned())
            })
            .state(UiState::INVALID, invalid)
            .a11y_from(
                A11yBinding::unspecified()
                    .labelled_by(self.label)
                    // A step rather than the `invalid` setter, because "valid" is the absence of
                    // the property rather than a value it can take, and a control that always
                    // declared one would be a control that is always announced as checked.
                    .step(move |a11y| {
                        if invalid() {
                            a11y.invalid(Invalid::True)
                        } else {
                            a11y
                        }
                    })
                    .described_by(move || {
                        Some(if invalid() {
                            self.message
                        } else {
                            self.description
                        })
                    }),
            )
    }
}

/// The field the calling scope is inside, when it is inside one.
///
/// What a control calls to join a [`Form`](crate::Form): the handle to bind, and the attributes to
/// spread. `None` outside a field, which is an ordinary answer — the same control is used on its
/// own everywhere else.
///
/// ```no_run
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
/// use zgui_ui::form::use_form_field;
///
/// /// A control that is part of a field when it is inside one, and ordinary when it is not.
/// #[component]
/// fn Name(value: RwSignal<String, zgui::reactive::LocalStorage>) -> impl IntoView {
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
#[must_use]
pub fn use_form_field() -> Option<FormFieldContext> {
    FormFieldContext::current()
}

/// One field of a [`Form`](crate::Form): a value, a rule, and the parts that describe it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::reactive::RwSignal;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui::form::Validator;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// let name = RwSignal::new_local(String::new());
/// let rule = Validator::new(move || name.get().is_empty().then(|| "Needed.".to_owned()));
/// view! {
///     Form {
///         FormField(name = "name", validate = rule) {
///             FormItem {
///                 FormLabel {"Name"}
///                 FormDescription {"As it appears on the card."}
///                 FormMessage()
///             }
///         }
///     }
/// }
/// # }
/// ```
///
/// The rule is checked whenever anything it reads changes, so there is no moment at which the
/// field's own idea of whether it is valid can disagree with its value.
#[component]
pub fn FormField(
    /// What this field is called, which is what a form reports it as.
    #[prop(into)]
    name: String,
    /// The rule its value has to pass. A field with no rule is always happy.
    #[prop(optional)]
    validate: Option<Validator>,
    /// Classes merged after the field's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The label, the control, the hint and the message.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, FormStyle::CSS);
    let form = FormContext::current();
    let rule = StoredValue::new_local(validate);
    let error = Signal::derive_local(move || rule.get_value().and_then(|rule| rule.check()));

    let context = FormFieldContext {
        error,
        touched: RwSignal::new_local(false),
        form,
        control: NodeRef::new(),
        label: NodeRef::new(),
        description: NodeRef::new(),
        message: NodeRef::new(),
    };
    provide_local_context(context);
    if let Some(form) = form {
        let _ = form.register(error, context.control);
    }

    let name = Rc::new(name);
    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-field"), move || {
            Some(name.to_string())
        })
        .attribute(zgui::view::AttrName::new("data-invalid"), move || {
            context.shown_error().is_some().then(|| "true".to_owned())
        });

    view! {
        box(
            class = "zui-form__field",
            on:focus_out = move |_| context.touch(),
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}
