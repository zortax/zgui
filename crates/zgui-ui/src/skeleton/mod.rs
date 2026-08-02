//! The shape of something that has not arrived yet.

mod style;

pub use crate::skeleton::style::SkeletonStyle;

use zgui::prelude::*;
use zgui::{component, view};

/// What the skeleton's rules are installed under.
const SHEET: &str = "zui-skeleton";

/// A pulsing block standing in for content still being loaded.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A card that has not arrived yet.
/// #[component]
/// fn Loading() -> impl IntoView {
///     view! {
///         column {
///             Skeleton(style:height = "20px", style:width = "60%")
///             Skeleton(style:height = "20px")
///         }
///     }
/// }
/// ```
///
/// # Size, and why there is no size prop
///
/// A skeleton stands in for something, so its size is that thing's size — which the caller knows
/// and this does not. It is an ordinary box with an ordinary width and height, set the way any
/// other box's are.
///
/// # What a reader is told
///
/// That the region is busy, and nothing else. The pulse means *wait* to somebody looking at it,
/// and `busy` is what means the same to somebody who is not.
#[component]
pub fn Skeleton(
    /// Classes merged after the skeleton's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, SkeletonStyle::CSS);
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-skeleton"), true)
        .a11y_from(A11yBinding::new(Role::GenericContainer).busy(true));

    view! { box(class = SkeletonStyle::CLASS, {..own}, {..attrs}, class = class) }
}
