//! A mark that turns while something is happening.

mod style;

pub use crate::spinner::style::SpinnerStyle;

use zgui::prelude::*;
use zgui::{component, view};

/// What the spinner's rules are installed under.
const SHEET: &str = "zui-spinner";

/// A turning ring saying that something is under way.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A button that is busy.
/// #[component]
/// fn Saving() -> impl IntoView {
///     view! { Button(disabled = true) {Spinner()text {"Saving"}} }
/// }
/// ```
///
/// # A spinner and a progress bar
///
/// A spinner says *something is happening*; a [`Progress`](crate::Progress) says *how much of it
/// has happened*. Use the bar whenever the answer is known, because a spinner beside a task with a
/// knowable length tells a user less than nothing — it tells them the length is unknowable.
///
/// # What a reader is told
///
/// That the region is busy, under a name. A turning ring means *wait* to somebody looking at it,
/// and a status with a label is what means the same to somebody who is not.
#[component]
pub fn Spinner(
    /// What is being waited for, for a reader.
    #[prop(into, default = String::from("Loading"))]
    label: String,
    /// Classes merged after the spinner's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, SpinnerStyle::CSS);
    // The stable class is what a control around the spinner selects on: a button narrows its side
    // padding when it holds a mark rather than a word, and a spinner is a mark.
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-spinner"), true)
        .a11y_from(A11yBinding::new(Role::Status).label(label).busy(true));

    view! { box(class = SpinnerStyle::CLASS, {..own}, {..attrs}, class = class) }
}
