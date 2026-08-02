//! The runs a code is broken into, and what goes between them.

use zgui::prelude::*;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::mark::MINUS;

use crate::input_otp::SHEET;
use crate::input_otp::style::InputOtpStyle;

/// One run of boxes inside an [`InputOtp`](crate::InputOtp).
///
/// The boxes in a run touch, and the runs are what the gaps are between — which is what makes a
/// code read as *three and three* rather than as six unrelated squares.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { InputOtp(length = 6, groups = vec![3, 3]) }
/// # }
/// ```
#[component]
pub fn InputOtpGroup(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The boxes in the run.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, InputOtpStyle::CSS);
    view! {
        box(class = "zui-otp__group", {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// The dash between two runs of an [`InputOtp`](crate::InputOtp).
///
/// It is decoration and says so: the code is one value on one control, and a reader that met a
/// separator here would be told about a punctuation mark nobody types.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::input_otp::{InputOtpSeparator, InputOtpSeparatorProps};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { InputOtpSeparator() }
/// # }
/// ```
#[component]
pub fn InputOtpSeparator(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, InputOtpStyle::CSS);
    view! {
        box(class = "zui-otp__separator", a11y:hidden = true, {..attrs}, class = class) {
            Icon(icon = MINUS)
        }
    }
}
