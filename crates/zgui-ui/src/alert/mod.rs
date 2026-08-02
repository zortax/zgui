//! Something the reader has to be told.

mod parts;
mod style;

pub use crate::alert::parts::{
    AlertDescription, AlertDescriptionProps, AlertTitle, AlertTitleProps,
};
pub use crate::alert::style::AlertStyle;

use zgui::prelude::*;
use zgui::{component, variants, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::status::{ALERT_TRIANGLE, INFO};

use crate::support::variant_attrs;

/// What the alert's rules are installed under.
pub(crate) const SHEET: &str = "zui-alert";

variants! {
    /// The axes an [`Alert`] varies along.
    pub AlertVariants {
        base: "zui-alert",
        variant: {
            Default => "zui-alert--default",
            Destructive => "zui-alert--destructive",
        } = Default,
    }
}

/// A message the reader is meant to notice, in place on the surface.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A warning about a card that is about to expire.
/// #[component]
/// fn Expiring() -> impl IntoView {
///     view! {
///         Alert(variant = AlertVariant::Destructive) {
///             AlertTitle {"Your card expires this month"}
///             AlertDescription {"Update it before the next invoice."}
///         }
///     }
/// }
/// ```
///
/// # What a reader is told, and when
///
/// An alert is a live region: content that appears in one interrupts whatever is being read, which
/// is right for a message about something that just went wrong and wrong for a message that was
/// on the surface when it opened. So an alert that is on screen from the start is an ordinary
/// region — pass `live=false` — and one that appears in response to something announces itself.
///
/// # The icon
///
/// Drawn from the variant and hidden from the accessibility tree, because it says the same thing
/// the title does and a reader that met both would say it twice. An alert with an icon of its own
/// passes one as a child and turns this one off.
///
/// The icon occupies a column of its own, and the alert says in `data-icon` whether that column
/// has any width — the title and the description line up down the second one either way.
///
/// # What the destructive variant changes
///
/// The writing, and nothing else. The surface and the border stay what every alert has: a red box
/// with a red edge and red text inside says the same thing three times, and the third one is the
/// only one anybody reads.
#[component]
pub fn Alert(
    /// How it looks, which also decides the icon.
    #[prop(default = AlertVariant::Default)]
    variant: AlertVariant,
    /// Whether appearing here interrupts what is being read.
    #[prop(default = true)]
    live: bool,
    /// Whether to draw the variant's own icon.
    #[prop(default = true)]
    icon: bool,
    /// Classes merged after the alert's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The title and the description.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, AlertStyle::CSS);
    let variants = AlertVariants { variant };
    let semantics = if live {
        A11yBinding::new(Role::Alert).live(zgui::vocab::Live::Polite)
    } else {
        A11yBinding::new(Role::Group)
    };
    // Which column the writing starts in depends on whether there is a mark to put in the first
    // one, and a sheet has no way to ask. So the answer is written down where it is already known.
    let own = variant_attrs(variants.classes(), variants.data_attributes())
        .attribute(
            zgui::view::AttrName::new("data-icon"),
            if icon { "true" } else { "false" },
        )
        .a11y_from(semantics);
    let mark = icon.then_some(match variant {
        AlertVariant::Default => INFO,
        AlertVariant::Destructive => ALERT_TRIANGLE,
    });
    let mark =
        mark.map(|mark| AnyView::new(view! { Icon(icon = mark, class = "zui-alert__icon") }));

    view! {
        surface(class = AlertStyle::CLASS, {..own}, {..attrs}, class = class) {
            {mark}
            {children.into_view_once()}
        }
    }
}
