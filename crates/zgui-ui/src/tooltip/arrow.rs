//! The point that ties a tooltip to what it names.

use zgui::prelude::*;
use zgui::{component, view};

use crate::tooltip::SHEET;
use crate::tooltip::style::TooltipStyle;

/// The small diamond on the edge of a tooltip facing its trigger.
///
/// Decorative, and declared so: it says which control the tooltip is about, and a reader already
/// knows because the control describes itself with the tooltip's text.
///
/// Which edge it goes on is the side the surface actually ended up on, which the positioner
/// publishes — so an arrow needs no prop and cannot disagree with the panel it is on.
///
/// [`TooltipContent`](crate::TooltipContent) draws one unless told not to.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui::tooltip::{TooltipArrow, TooltipArrowProps};
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Tooltip {
///         TooltipTrigger {Button {"?"}}
///         TooltipContent(arrow = false) {"Applies here only" TooltipArrow()}
///     }
/// }
/// # }
/// ```
#[component]
pub fn TooltipArrow(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, TooltipStyle::CSS);
    view! { box(class = "zui-tooltip__arrow", a11y:hidden = true, {..attrs}, class = class) }
}
