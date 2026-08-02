//! Lengths, as the cascade leaves them.

use zgui_geom::CssPx;

/// An absolute length in CSS pixels, which is what every absolute unit computes to.
pub use style::values::computed::Length;
/// A length that additionally carries a percentage of something not yet known.
///
/// The percentage is left unresolved because the basis is not a property of the value: every text
/// property that accepts one measures it against something different — `letter-spacing` against the
/// font size, `word-spacing` against the advance of a space in whichever face was chosen, and
/// `text-indent` against the width the block was laid out in. Resolution therefore belongs where
/// each of those is known, which is used-value time and never here.
pub use style::values::computed::LengthPercentage;
/// A [`Length`] the grammar forbids from being negative.
pub use style::values::computed::NonNegativeLength;
/// A number the grammar forbids from being negative, which is what a unitless `line-height` is.
pub use style::values::computed::NonNegativeNumber;
/// The wrapper the grammar's non-negative types are built from.
///
/// A reader that has to *construct* one — a test varying a single length, or a conversion that has
/// to put a length back where a non-negative one is expected — needs to name it.
pub use style::values::generics::NonNegative;

/// A length in this framework's own unit.
///
/// ```
/// use zgui_css::values::length::{Length, to_css_px};
/// use zgui_geom::CssPx;
///
/// assert_eq!(to_css_px(Length::new(12.0)), CssPx(12.0));
/// ```
pub fn to_css_px(length: Length) -> CssPx {
    CssPx(length.px())
}

/// Evaluates a length-or-percentage at one basis, in this framework's own unit.
///
/// The basis is whatever the property in hand measures its percentage against, and no two text
/// properties agree on what that is — so this takes it as an argument and makes no claim about
/// which one is right.
///
/// ```
/// use zgui_css::values::length::{Length, LengthPercentage, evaluate_at, percent};
/// use zgui_geom::CssPx;
///
/// let absolute = LengthPercentage::new_length(Length::new(3.0));
/// assert_eq!(evaluate_at(&absolute, CssPx(16.0)), CssPx(3.0));
///
/// // A half of whatever the basis turns out to be.
/// assert_eq!(evaluate_at(&percent(0.5), CssPx(16.0)), CssPx(8.0));
/// ```
pub fn evaluate_at(value: &LengthPercentage, basis: CssPx) -> CssPx {
    CssPx(value.resolve(Length::new(basis.0)).px())
}

/// Builds a length-or-percentage holding only a percentage, where `0.25` is twenty-five percent.
pub fn percent(fraction: f32) -> LengthPercentage {
    LengthPercentage::new_percent(style::values::computed::Percentage(fraction))
}
