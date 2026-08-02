//! What a box asks to be: its sizes, its insets and the boxes it makes.

/// The computed value of `aspect-ratio`.
pub use style::values::computed::AspectRatio as AspectRatioValue;
/// The computed value of `flex-basis`.
pub use style::values::computed::FlexBasis as FlexBasisValue;
/// The computed value of `top`, `right`, `bottom` and `left`.
pub use style::values::computed::Inset as InsetValue;
/// The computed value of one margin side, which may be `auto`.
pub use style::values::computed::Margin as MarginValue;
/// The computed value of `max-width` and `max-height`, which additionally may be `none`.
pub use style::values::computed::MaxSize as MaxSizeValue;
/// The computed value of one padding side, which the grammar forbids from being negative.
pub use style::values::computed::NonNegativeLengthPercentage as PaddingValue;
/// A `<ratio>`: two non-negative numbers whose quotient is the ratio.
pub use style::values::computed::Ratio as RatioValue;
/// The computed value of `width`, `height`, `min-width` and `min-height`.
///
/// A size is a length, a percentage of the containing block, or one of the keywords that make the
/// answer depend on the content — so it is not a number, and it cannot become one until the
/// containing block is known.
pub use style::values::computed::Size as SizeValue;
/// The computed value of `z-index`.
pub use style::values::computed::ZIndex as ZIndexValue;
/// The computed value of `row-gap` and `column-gap`.
pub use style::values::computed::length::NonNegativeLengthPercentageOrNormal as GapValue;
/// The ratio half of an `aspect-ratio`, which may be absent when only `auto` was written.
pub use style::values::generics::position::PreferredRatio;

/// The computed value of `box-sizing`.
pub use style::computed_values::box_sizing::T as BoxSizingValue;
/// The computed value of `visibility`, whose `collapse` value removes a flex item's box.
pub use style::computed_values::visibility::T as VisibilityValue;
/// The computed value of `clear`.
pub use style::values::computed::Clear as ClearValue;
/// The computed value of `display`, which is a packed pair of an outer and an inner display type.
pub use style::values::computed::Display as DisplayValue;
/// The computed value of `overflow-x` and `overflow-y`.
pub use style::values::computed::Overflow as OverflowValue;
/// The computed value of `position`.
pub use style::values::computed::PositionProperty as PositionValue;
/// The computed value of `float`.
pub use style::values::computed::box_::Float as FloatValue;
/// What formatting context a box establishes for its children: the inner half of `display`.
pub use style::values::specified::box_::DisplayInside;
/// How a box participates in its parent's formatting context: the outer half of `display`.
pub use style::values::specified::box_::DisplayOutside;
