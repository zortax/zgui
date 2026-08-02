//! A box that keeps its width and height in a fixed proportion.

mod style;

pub use crate::aspect_ratio::style::AspectRatioStyle;

use zgui::prelude::*;
use zgui::{component, view};

/// What the box's rules are installed under.
const SHEET: &str = "zui-aspect-ratio";

/// A box that is as wide as it can be and as tall as its ratio says.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A picture that stays widescreen however wide the column is.
/// #[component]
/// fn Cover() -> impl IntoView {
///     view! { AspectRatio(ratio = 16.0 / 9.0) {Skeleton()} }
/// }
/// ```
///
/// # Why the ratio and not a height
///
/// A height in pixels is a height that is wrong at every width but one, and a grid of pictures
/// laid out that way tears the moment the window is resized. Giving the proportion instead lets
/// the layout solve for the height at whatever width the box ends up, so a row of covers stays a
/// row of covers.
///
/// # What a reader is told
///
/// Nothing. The box is a shape and not a thing, so it carries no role of its own and whatever is
/// inside it is announced as if the box were not there.
#[component]
pub fn AspectRatio(
    /// Width divided by height: `16.0 / 9.0` is widescreen, `1.0` is square.
    #[prop(default = 1.0)]
    ratio: f32,
    /// Classes merged after the box's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What fills it.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, AspectRatioStyle::CSS);
    let own = Attrs::new().custom_property(
        zgui::view::CustomPropertyName::new("zui-aspect-ratio"),
        move || Some(format!("{ratio}")),
    );

    view! {
        box(class = AspectRatioStyle::CLASS, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
