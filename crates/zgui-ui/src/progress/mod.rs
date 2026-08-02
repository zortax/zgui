//! How far along something is.

mod style;

pub use crate::progress::style::ProgressStyle;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};

use crate::support::Bound;

/// What the progress bar's rules are installed under.
const SHEET: &str = "zui-progress";

/// A bar showing how far along something is.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// An upload, and how far it has got.
/// #[component]
/// fn Upload() -> impl IntoView {
///     let done = RwSignal::new_local(Some(35.0));
///     view! { Progress(value = done, max = 100.0) }
/// }
/// ```
///
/// # A value that is not known yet
///
/// `value` is an `Option`, and `None` is the indeterminate bar: a stripe that slides rather than a
/// fill that grows. It is a different thing to say and it says it to a reader as well — a
/// determinate bar reports a number, and an indeterminate one reports none rather than reporting
/// zero, which would be a claim that nothing has happened.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Waiting() -> impl IntoView {
/// view! { Progress(label = "Connecting") }
/// # }
/// ```
#[component]
pub fn Progress(
    /// How far along it is, or nothing when that is not known.
    #[prop(into, default = Signal::stored_local(None))]
    value: Signal<Option<f64>, LocalStorage>,
    /// The value that counts as finished.
    #[prop(default = 100.0)]
    max: f64,
    /// What the bar is measuring, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after the bar's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, ProgressStyle::CSS);
    let bound = Bound::new(0.0, max, 0.0);

    let mut semantics = A11yBinding::new(Role::ProgressIndicator)
        .step(move |a11y| match value.get() {
            Some(value) => a11y.numeric_value(value.clamp(0.0, max)),
            None => a11y,
        })
        .step(move |a11y| a11y.numeric_range(0.0, max));
    if let Some(text) = label {
        semantics = semantics.label(text);
    }

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-progress"), true)
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(match value.get() {
                Some(_) => "determinate".to_owned(),
                None => "indeterminate".to_owned(),
            })
        })
        .custom_property(
            zgui::view::CustomPropertyName::new("zui-progress-fraction"),
            move || {
                value
                    .get()
                    .map(|value| format!("{:.4}%", bound.fraction(value) * 100.0))
            },
        )
        .a11y_from(semantics);

    view! {
        box(class = ProgressStyle::CLASS, {..own}, {..attrs}, class = class) {
            box(class = "zui-progress__fill")
        }
    }
}
