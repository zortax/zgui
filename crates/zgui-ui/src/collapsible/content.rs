//! The part of a disclosure that is shown and hidden, and how tall it turns out to be.

use zgui::prelude::*;
use zgui::{component, view};

use crate::collapsible::style::CollapsibleStyle;
use crate::collapsible::{CollapsibleContext, SHEET};

/// The part of a [`Collapsible`](crate::Collapsible) that is shown and hidden.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Collapsible(default_open = true) {
///         CollapsibleTrigger {"Delivery details"}
///         CollapsibleContent {text {"Arrives Thursday, signed for."}}
///     }
/// }
/// # }
/// ```
///
/// # How it slides
///
/// Two elements: an outer one that is clipped and animates its `height`, and an inner one that is
/// never clipped and is therefore exactly as tall as the content. The inner one is measured
/// through the observation channel, and what it measures is published on the outer one as
/// `--zui-collapsible-height`, which is what the sheet animates to.
///
/// That is the whole reason for the second element. A style sheet cannot animate to `auto`, and a
/// pixel height written into a sheet is wrong the first time the content changes — a translated
/// label, a longer line, a larger type scale. Here nothing is written down: the number is whatever
/// the last layout said, it is re-published when that changes, and a caller who overrides the
/// duration or the curve in CSS changes the animation without touching any of it.
///
/// The measurement is in CSS pixels, because that is the unit a style sheet is written in. The
/// observation reports device pixels, and the two differ by the window's scale on every display
/// that is not exactly 1×.
#[component]
pub fn CollapsibleContent(
    /// Classes merged after the content's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What is shown and hidden.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, CollapsibleStyle::CSS);
    let context = CollapsibleContext::current();
    let open = move || context.is_some_and(|context| context.is_open());

    let inner = NodeRef::new();
    let measured = inner.observe_border_box();
    let scale = move || {
        let scale = inner.scale();
        if scale > 0.0 { scale } else { 1.0 }
    };

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(if open() { "open" } else { "closed" }.to_owned())
        })
        .custom_property(
            zgui::view::CustomPropertyName::new("zui-collapsible-height"),
            move || {
                measured
                    .get()
                    .map(|box_| format!("{}px", box_.size.height.0 / scale()))
            },
        )
        // Hidden content is hidden from a reader too. Without this the section would be clipped to
        // nothing on screen and read out in full, which is the same defect as a caption that does
        // not match the picture.
        .a11y_from(A11yBinding::unspecified().hidden(move || !open()));

    view! {
        box(
            class = "zui-collapsible__content",
            node_ref = context.map(CollapsibleContext::content).unwrap_or_default(),
            {..own},
            {..attrs},
            class = class
        ) {
            box(class = "zui-collapsible__measure", node_ref = inner) {
                {children.into_view_once()}
            }
        }
    }
}
