//! The track the slides sit on, and how far it has travelled.

use zgui::prelude::*;
use zgui::{component, view};
use zgui_ui_primitives::Orientation;

use crate::carousel::context::CarouselContext;
use crate::carousel::style::CarouselStyle;
use crate::carousel::{SHEET, style};

/// The track a [`Carousel`](crate::Carousel)'s slides sit on.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Carousel {
///         CarouselContent {CarouselItem {text {"One"}}}
///     }
/// }
/// # }
/// ```
///
/// # How far it moves
///
/// The showing slide reaches the sheet as `--zui-carousel-offset`: how far the track has to move
/// for that slide to sit against the viewport's leading edge, as a negative length. The sheet
/// turns that into one offset and nothing else.
///
/// A length rather than a count of viewports, because the distance is *measured* off the slides:
/// a track divided by how many slides it holds steps by their average, which overshoots every
/// narrow slide and stops short of every wide one, and a viewport holding two slides at once has
/// no whole number of viewports that is one slide.
///
/// Which slide is showing is published beside it as `--zui-carousel-index`, a plain number, for a
/// sheet that wants to count rather than to measure.
#[component]
pub fn CarouselContent(
    /// Classes merged after the track's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The slides.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, CarouselStyle::CSS);
    let context = CarouselContext::current();
    let track = NodeRef::new();
    let measured = track.observe_border_box();
    let vertical = context
        .map(CarouselContext::orientation)
        .is_some_and(|orientation| matches!(orientation, Orientation::Vertical));

    // How far the track has to move, in CSS pixels, for the showing slide to sit against the
    // viewport's leading edge. Two readings taken in the same space and subtracted: the showing
    // slide's leading edge, less the track's own. A difference, because both are in window
    // coordinates and the track has already been moved by whatever this last answered — which
    // moves the track and every slide on it together, so the distance between them is the one
    // number the offset itself cannot disturb.
    let travelled = move || {
        let start = context?.start_of_showing()?;
        let box_ = measured.get()?;
        let from = if vertical {
            box_.origin.y.0
        } else {
            box_.origin.x.0
        };
        // Device pixels are what geometry is observed in and CSS pixels are what a sheet is
        // written in; the two differ by the window's scale on every display that is not exactly 1×.
        let scale = track.scale();
        let distance = (start - from) / if scale > 0.0 { scale } else { 1.0 };
        distance.is_finite().then_some(distance)
    };

    let own = Attrs::new()
        .custom_property(
            zgui::view::CustomPropertyName::new(style::INDEX),
            move || Some(context.map_or(0, CarouselContext::index).to_string()),
        )
        .custom_property(
            zgui::view::CustomPropertyName::new(style::OFFSET),
            move || travelled().map(|distance| format!("{}px", -distance)),
        );

    view! {
        box(class = "zui-carousel__viewport") {
            box(class = "zui-carousel__track", node_ref = track, {..own}, {..attrs}, class = class) {
                {children.into_view_once()}
            }
        }
    }
}
