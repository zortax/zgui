//! One slide.

use zgui::prelude::*;
use zgui::{component, view};

use crate::carousel::SHEET;
use crate::carousel::context::CarouselContext;
use crate::carousel::style::CarouselStyle;

/// One slide of a [`CarouselContent`](crate::CarouselContent).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Carousel {CarouselContent {
///         CarouselItem {text {"One"}}
///         CarouselItem {text {"Two"}}
///     }}
/// }
/// # }
/// ```
///
/// Announced as slide *n* of *m*, counted from where it actually sits rather than from a number
/// written at the call site — so a slide behind a conditional does not make every slide after it
/// lie about its position.
///
/// # How wide it is
///
/// The width of the viewport, until a caller says otherwise with a class of their own. A narrower
/// basis puts several slides in view at once and a per-slide one gives them different widths;
/// either way a step still brings exactly one slide to the front, because how far the track moves
/// is measured off this element rather than assumed from it.
#[component]
pub fn CarouselItem(
    /// Classes merged after the slide's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the slide holds.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, CarouselStyle::CSS);
    let context = CarouselContext::current();
    let element = NodeRef::new();
    let id = context.map(|carousel| carousel.register(element));
    let position = move || {
        let (context, id) = (context?, id?);
        context.position_of(id)
    };
    let showing = move || context.is_some_and(|carousel| position() == Some(carousel.index()));

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(if showing() { "active" } else { "inactive" }.to_owned())
        })
        .a11y_from(
            A11yBinding::new(Role::Group)
                .role_description("slide")
                .hidden(move || !showing())
                .step(move |a11y| match (position(), context) {
                    (Some(at), Some(carousel)) => a11y.set_position(at + 1, carousel.count()),
                    _ => a11y,
                }),
        );

    view! {
        box(class = "zui-carousel__item", node_ref = element, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
