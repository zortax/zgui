//! What every component here does the same way.

mod edit;
mod value;

pub use crate::support::edit::{Edit, apply, key_edit};
pub use crate::support::value::{Bound, clamp_to_step};

use zgui::prelude::*;
use zgui::view::AttrName;

/// The attributes a variants table contributes: its class list, and one `data-` attribute per axis.
///
/// Both, rather than one or the other, because they answer different questions. The classes are
/// what a rule is written against; the `data-` attributes are what a rule *selects* on when the
/// answer is a choice rather than a flag — `[data-size="sm"]` reads as the axis it belongs to, and
/// a transcript of a frame says which variant was mounted without anybody decoding a class name.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::variants;
/// use zgui_ui::support::variant_attrs;
///
/// variants! {
///     /// A worked example.
///     pub ChipVariants {
///         base: "chip",
///         tone: { Plain => "", Loud => "chip--loud" } = Plain,
///     }
/// }
///
/// let chip = ChipVariants { tone: ChipTone::Loud };
/// let attrs = variant_attrs(chip.classes(), chip.data_attributes());
/// assert_eq!(attrs.classes().len(), 2);
/// assert_eq!(attrs.entries().len(), 1, "one attribute per axis");
/// ```
#[must_use]
pub fn variant_attrs<const N: usize>(
    classes: Classes,
    data: [(&'static str, &'static str); N],
) -> Attrs {
    let mut attrs = Attrs::new().classes_from(classes);
    for (name, value) in data {
        attrs = attrs.attribute(AttrName::new(name), value);
    }
    attrs
}
