//! The two controls that step the track.

use zgui::prelude::*;
use zgui::vocab::UiState;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::arrow::{ARROW_LEFT, ARROW_RIGHT};

use crate::carousel::SHEET;
use crate::carousel::context::CarouselContext;
use crate::carousel::style::CarouselStyle;

/// The control that goes back one slide.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Carousel {CarouselPrevious()} }
/// # }
/// ```
///
/// Disabled at the first slide, unless the carousel wraps — so the control tells the truth about
/// whether there is anything behind it rather than doing nothing when pressed.
///
/// It hangs in the margin beside the strip rather than sitting in the row with it, and in a
/// vertical carousel it is the same arrow turned a quarter.
#[component]
pub fn CarouselPrevious(
    /// What it is called, for a reader.
    #[prop(into, default = String::from("Previous slide"))]
    label: String,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, CarouselStyle::CSS);
    let context = CarouselContext::current();
    let out = move || !context.is_some_and(CarouselContext::has_previous);

    let own = Attrs::new()
        .state(UiState::DISABLED, out)
        .a11y_from(A11yBinding::new(Role::Button).label(label).disabled(out));

    view! {
        control(
            class = "zui-carousel__arrow",
            class = "zui-carousel__arrow--previous",
            tabindex = {Focus::Sequential},
            on:click = move |_| { if let Some(context) = context { context.previous() } },
            {..own},
            {..attrs},
            class = class
        ) {
            Icon(icon = ARROW_LEFT)
        }
    }
}

/// The control that goes on one slide.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Carousel {CarouselNext()} }
/// # }
/// ```
///
/// Disabled at the last slide, unless the carousel wraps.
#[component]
pub fn CarouselNext(
    /// What it is called, for a reader.
    #[prop(into, default = String::from("Next slide"))]
    label: String,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, CarouselStyle::CSS);
    let context = CarouselContext::current();
    let out = move || !context.is_some_and(CarouselContext::has_next);

    let own = Attrs::new()
        .state(UiState::DISABLED, out)
        .a11y_from(A11yBinding::new(Role::Button).label(label).disabled(out));

    view! {
        control(
            class = "zui-carousel__arrow",
            class = "zui-carousel__arrow--next",
            tabindex = {Focus::Sequential},
            on:click = move |_| { if let Some(context) = context { context.next() } },
            {..own},
            {..attrs},
            class = class
        ) {
            Icon(icon = ARROW_RIGHT)
        }
    }
}
