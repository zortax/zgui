//! One thing being asked for, with everything a person needs in order to answer it.

mod frame;
mod style;
mod text;

pub use crate::field::frame::{
    FieldContent, FieldContentProps, FieldGroup, FieldGroupProps, FieldLegend, FieldLegendProps,
    FieldLegendVariant, FieldLegendVariants, FieldSet, FieldSetProps,
};
pub use crate::field::style::{FieldGroupStyle, FieldStyle, FieldTextStyle};
pub use crate::field::text::{
    FieldDescription, FieldDescriptionProps, FieldError, FieldErrorProps, FieldLabel,
    FieldLabelProps, FieldSeparator, FieldSeparatorProps, FieldTitle, FieldTitleProps,
};

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, variants, view};

use crate::support::variant_attrs;

/// What a field's own rules are installed under.
pub(crate) const SHEET: &str = "zui-field";

/// What the rules for a set and a group of fields are installed under.
pub(crate) const GROUP_SHEET: &str = "zui-field-group";

/// What the rules for a field's writing are installed under.
pub(crate) const TEXT_SHEET: &str = "zui-field-text";

variants! {
    /// The axes a [`Field`] varies along.
    pub FieldVariants {
        base: "zui-field",
        orientation: {
            Vertical => "",
            Horizontal => "zui-field--horizontal",
        } = Vertical,
    }
}

/// A control, its label, its description and whatever is wrong with it, laid out together.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Somewhere to put an address.
/// #[component]
/// fn Email() -> impl IntoView {
///     let address = RwSignal::new_local(String::new());
///     view! {
///         Field {
///             FieldLabel {"Email"}
///             Input(value = address)
///             FieldDescription {"We only use this to send you receipts."}
///         }
///     }
/// }
/// ```
///
/// # Vertical and horizontal
///
/// Vertical is the ordinary way round: the label above the control, which is what a form of text
/// fields wants. Horizontal puts the label first and the control after it on one line, which is
/// what a switch or a checkbox wants — those read as *this thing, on or off* rather than as a
/// question with an answer underneath.
///
/// # A field and a form field
///
/// This one is layout: it arranges the pieces and marks itself when the answer is wrong.
/// [`FormField`](crate::FormField) is the other thing — it owns a value, validates it and reports
/// to the [`Form`](crate::Form) around it. They compose: a form field's contents are usually a
/// field.
///
/// # What a reader is told
///
/// That the pieces are a group, so the label, the description and the error stay attached to the
/// control for somebody who is not looking at the layout that attaches them.
#[component]
pub fn Field(
    /// Which way the label and the control sit.
    #[prop(default = FieldOrientation::Vertical)]
    orientation: FieldOrientation,
    /// Whether the answer is wrong, which colours the whole field.
    #[prop(into, default = Signal::stored_local(false))]
    invalid: Signal<bool, LocalStorage>,
    /// Whether the question is out of action, which fades the label and the writing with the
    /// control.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Classes merged after the field's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The label, the control, and what goes with them.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, FieldStyle::CSS);
    let variants = FieldVariants { orientation };
    let own = variant_attrs(variants.classes(), variants.data_attributes())
        .attribute(zgui::view::AttrName::new("data-invalid"), move || {
            Some(if invalid.get() { "true" } else { "false" }.to_owned())
        })
        .attribute(zgui::view::AttrName::new("data-disabled"), move || {
            Some(if disabled.get() { "true" } else { "false" }.to_owned())
        })
        .a11y_from(A11yBinding::new(Role::Group).disabled(move || disabled.get()));

    view! {
        box(class = FieldStyle::CLASS, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
