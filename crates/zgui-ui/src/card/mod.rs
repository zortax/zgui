//! A surface with a heading, a body and a row of controls.

mod body;
mod head;
mod style;

pub use crate::card::body::{CardContent, CardContentProps, CardFooter, CardFooterProps};
pub use crate::card::head::{
    CardAction, CardActionProps, CardDescription, CardDescriptionProps, CardHeader,
    CardHeaderContext, CardHeaderProps, CardTitle, CardTitleProps,
};
pub use crate::card::style::CardStyle;

use zgui::prelude::*;
use zgui::{component, view};

/// What the card's rules are installed under.
pub(crate) const SHEET: &str = "zui-card";

/// A raised surface holding one thing.
///
/// The pieces are components rather than props, because a card is a layout and not a form: some
/// have a title and no description, some a footer and no header, and some a picture where the
/// header would be. Every piece is optional and they compose in whatever order they are written.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
/// use zgui_ui::card::{CardAction, CardActionProps};
///
/// /// A card asking to be paid.
/// #[component]
/// fn Invoice() -> impl IntoView {
///     view! {
///         Card {
///             CardHeader {
///                 CardTitle {"March"}
///                 CardDescription {"Due on the 28th"}
///                 CardAction {Button {"Pay now"}}
///             }
///             CardContent {text {"£42.00"}}
///             CardFooter {Button {"Pay"}}
///         }
///     }
/// }
/// ```
///
/// # What a reader is told
///
/// That it is a group, which is what makes the pieces inside it one thing rather than several
/// loose ones. A card that is the whole subject of the surface — an article, a dialog's body —
/// says so with `a11y:role` rather than being given a prop for every role a box can have.
#[component]
pub fn Card(
    /// Classes merged after the card's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The pieces.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, CardStyle::CSS);
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-card"), true)
        .a11y_from(A11yBinding::new(Role::Group));

    view! {
        surface(class = CardStyle::CLASS, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
