//! A line between two groups of things.

mod style;

pub use crate::separator::style::SeparatorStyle;

use zgui::prelude::*;
use zgui::{component, variants, view};

use crate::support::variant_attrs;

/// What the separator's rules are installed under.
const SHEET: &str = "zui-separator";

variants! {
    /// The axes a [`Separator`] varies along.
    pub SeparatorVariants {
        base: "zui-separator",
        orientation: {
            Horizontal => "zui-separator--horizontal",
            Vertical => "zui-separator--vertical",
        } = Horizontal,
    }
}

/// A rule between two groups of things.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Two sections with a line between them.
/// #[component]
/// fn Sections() -> impl IntoView {
///     view! {
///         column {
///             text {"Profile"}
///             Separator()
///             text {"Billing"}
///         }
///     }
/// }
/// ```
///
/// # Decorative, and why it is the default
///
/// Most rules are drawing. They separate things that are already separated by their headings, and
/// announcing one is noise between two sections a reader has already been told apart. So a
/// separator is hidden from the accessibility tree unless `decorative` is turned off, and then it
/// is a splitter that reports which way it lies.
#[component]
pub fn Separator(
    /// Which way it lies.
    #[prop(default = SeparatorOrientation::Horizontal)]
    orientation: SeparatorOrientation,
    /// Whether it is drawing rather than structure.
    #[prop(default = true)]
    decorative: bool,
    /// Classes merged after the separator's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, SeparatorStyle::CSS);
    let variants = SeparatorVariants { orientation };
    let semantics = if decorative {
        A11yBinding::new(Role::GenericContainer).hidden(true)
    } else {
        A11yBinding::new(Role::Splitter).orientation(match orientation {
            SeparatorOrientation::Horizontal => zgui::vocab::Orientation::Horizontal,
            SeparatorOrientation::Vertical => zgui::vocab::Orientation::Vertical,
        })
    };
    let own = variant_attrs(variants.classes(), variants.data_attributes()).a11y_from(semantics);

    view! { box(class = SeparatorStyle::CLASS, {..own}, {..attrs}, class = class) }
}
